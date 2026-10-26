import ChatOSWorkerRuntime
import Dispatch
import Foundation
import IOKit.ps

private func chatOSPowerSourceDidChange(_ context: UnsafeMutableRawPointer?) {
    guard let context else { return }
    Unmanaged<ClientMachineResourcePressureMonitor>
        .fromOpaque(context)
        .takeUnretainedValue()
        .scheduleRefresh()
}

/// Converts process-wide macOS pressure signals into admission capacity changes. The monitor does
/// not own work or terminate processes; it only updates the claim-before-spawn Governor.
final class ClientMachineResourcePressureMonitor: @unchecked Sendable {
    private struct BatteryState {
        let onBatteryPower: Bool
        let level: Double?
    }

    private let governor: ClientMachineResourceGovernor
    private let stateQueue = DispatchQueue(
        label: "com.chatos.machine-resource-pressure",
        qos: .utility
    )
    private var memoryPressure: ClientMachineMemoryPressure = .normal
    private var lastSubmittedState: ClientMachinePressureState?
    private var submissionTask: Task<Void, Never>?

    private var started = false
    private var memoryPressureSource: (any DispatchSourceMemoryPressure)?
    private var thermalObserver: NSObjectProtocol?
    private var lowPowerObserver: NSObjectProtocol?
    private var powerSourceRunLoopSource: CFRunLoopSource?

    init(governor: ClientMachineResourceGovernor) {
        self.governor = governor
    }

    @MainActor
    func start() {
        guard !started else { return }
        started = true

        let memorySource = DispatchSource.makeMemoryPressureSource(
            eventMask: [.normal, .warning, .critical],
            queue: stateQueue
        )
        memorySource.setEventHandler { [weak self, weak memorySource] in
            guard let self, let event = memorySource?.data else { return }
            if event.contains(.critical) {
                memoryPressure = .critical
            } else if event.contains(.warning) {
                memoryPressure = .warning
            } else if event.contains(.normal) {
                memoryPressure = .normal
            }
            refreshPressureState()
        }
        memoryPressureSource = memorySource
        memorySource.activate()

        thermalObserver = NotificationCenter.default.addObserver(
            forName: ProcessInfo.thermalStateDidChangeNotification,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.scheduleRefresh()
        }
        lowPowerObserver = NotificationCenter.default.addObserver(
            forName: Notification.Name.NSProcessInfoPowerStateDidChange,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.scheduleRefresh()
        }

        let context = Unmanaged.passUnretained(self).toOpaque()
        if let source = IOPSNotificationCreateRunLoopSource(
            chatOSPowerSourceDidChange,
            context
        )?.takeRetainedValue() {
            powerSourceRunLoopSource = source
            CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        }
        scheduleRefresh()
    }

    @MainActor
    func stop() {
        guard started else { return }
        started = false

        memoryPressureSource?.setEventHandler(handler: nil)
        memoryPressureSource?.cancel()
        memoryPressureSource = nil
        if let thermalObserver {
            NotificationCenter.default.removeObserver(thermalObserver)
            self.thermalObserver = nil
        }
        if let lowPowerObserver {
            NotificationCenter.default.removeObserver(lowPowerObserver)
            self.lowPowerObserver = nil
        }
        if let powerSourceRunLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), powerSourceRunLoopSource, .commonModes)
            self.powerSourceRunLoopSource = nil
        }
    }

    fileprivate func scheduleRefresh() {
        stateQueue.async { [weak self] in
            self?.refreshPressureState()
        }
    }

    private func refreshPressureState() {
        let processInfo = ProcessInfo.processInfo
        let battery = Self.readBatteryState()
        let state = ClientMachinePressureState(
            memory: memoryPressure,
            thermal: Self.thermalPressure(processInfo.thermalState),
            lowPowerModeEnabled: processInfo.isLowPowerModeEnabled,
            onBatteryPower: battery.onBatteryPower,
            batteryLevel: battery.level
        )
        guard state != lastSubmittedState else { return }
        lastSubmittedState = state

        let previous = submissionTask
        let governor = governor
        submissionTask = Task {
            await previous?.value
            await governor.updatePressureState(state)
        }
    }

    private static func thermalPressure(
        _ state: ProcessInfo.ThermalState
    ) -> ClientMachineThermalPressure {
        switch state {
        case .nominal: .nominal
        case .fair: .fair
        case .serious: .serious
        case .critical: .critical
        @unknown default: .serious
        }
    }

    private static func readBatteryState() -> BatteryState {
        guard let snapshot = IOPSCopyPowerSourcesInfo()?.takeRetainedValue(),
              let sourceList = IOPSCopyPowerSourcesList(snapshot)?.takeRetainedValue()
                as? [AnyObject] else {
            return .init(onBatteryPower: false, level: nil)
        }

        var foundBattery = false
        var onBatteryPower = false
        var minimumLevel: Double?
        for source in sourceList {
            guard let values = IOPSGetPowerSourceDescription(
                snapshot,
                source
            )?.takeUnretainedValue() as? [String: Any],
            values[kIOPSTypeKey] as? String == kIOPSInternalBatteryType else {
                continue
            }
            foundBattery = true
            if values[kIOPSPowerSourceStateKey] as? String == kIOPSBatteryPowerValue {
                onBatteryPower = true
            }
            if let current = values[kIOPSCurrentCapacityKey] as? NSNumber,
               let maximum = values[kIOPSMaxCapacityKey] as? NSNumber,
               maximum.doubleValue > 0 {
                let level = min(1, max(0, current.doubleValue / maximum.doubleValue))
                minimumLevel = minimumLevel.map { min($0, level) } ?? level
            }
        }
        return .init(
            onBatteryPower: foundBattery && onBatteryPower,
            level: foundBattery ? minimumLevel : nil
        )
    }
}
