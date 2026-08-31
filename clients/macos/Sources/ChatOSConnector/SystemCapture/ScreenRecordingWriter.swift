import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit

final class ScreenRecordingWriter: NSObject, SCStreamOutput, @unchecked Sendable {
    private let outputURL: URL
    private let writer: AVAssetWriter
    private let videoInput: AVAssetWriterInput
    private let audioInput: AVAssetWriterInput?
    private let processingQueue: DispatchQueue
    private var sessionStartTime: CMTime?
    private var isFinishing = false
    private var terminalError: Error?

    init(
        outputURL: URL,
        width: Int,
        height: Int,
        capturesAudio: Bool,
        processingQueue: DispatchQueue
    ) throws {
        self.outputURL = outputURL
        self.processingQueue = processingQueue
        self.writer = try AVAssetWriter(outputURL: outputURL, fileType: .mov)
        let pixelCount = max(1, width * height)
        let bitRate = min(28_000_000, max(5_000_000, pixelCount * 5))
        self.videoInput = AVAssetWriterInput(
            mediaType: .video,
            outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.h264,
                AVVideoWidthKey: width,
                AVVideoHeightKey: height,
                AVVideoCompressionPropertiesKey: [
                    AVVideoAverageBitRateKey: bitRate,
                    AVVideoExpectedSourceFrameRateKey: 30,
                    AVVideoMaxKeyFrameIntervalKey: 60,
                    AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
                ],
            ]
        )
        videoInput.expectsMediaDataInRealTime = true
        guard writer.canAdd(videoInput) else {
            throw NativeScreenRecordingError.writer("The video encoder could not be configured.")
        }
        writer.add(videoInput)

        if capturesAudio {
            let input = AVAssetWriterInput(
                mediaType: .audio,
                outputSettings: [
                    AVFormatIDKey: kAudioFormatMPEG4AAC,
                    AVSampleRateKey: 48_000,
                    AVNumberOfChannelsKey: 2,
                    AVEncoderBitRateKey: 192_000,
                ]
            )
            input.expectsMediaDataInRealTime = true
            guard writer.canAdd(input) else {
                throw NativeScreenRecordingError.writer("The system audio encoder could not be configured.")
            }
            writer.add(input)
            self.audioInput = input
        } else {
            self.audioInput = nil
        }
        super.init()
    }

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of outputType: SCStreamOutputType
    ) {
        guard !isFinishing,
              terminalError == nil,
              sampleBuffer.isValid,
              CMSampleBufferDataIsReady(sampleBuffer) else { return }

        switch outputType {
        case .screen:
            appendVideo(sampleBuffer)
        case .audio:
            appendAudio(sampleBuffer)
        case .microphone:
            break
        @unknown default:
            break
        }
    }

    func finish() async throws -> URL {
        try await withCheckedThrowingContinuation { continuation in
            processingQueue.async { [self] in
                guard !isFinishing else {
                    continuation.resume(throwing: NativeScreenRecordingError.writer("The recording is already finishing."))
                    return
                }
                isFinishing = true
                if let terminalError {
                    writer.cancelWriting()
                    continuation.resume(throwing: terminalError)
                    return
                }
                guard sessionStartTime != nil else {
                    writer.cancelWriting()
                    continuation.resume(throwing: NativeScreenRecordingError.writer("The recording did not receive any video frames."))
                    return
                }
                videoInput.markAsFinished()
                audioInput?.markAsFinished()
                writer.finishWriting { [self] in
                    if writer.status == .completed {
                        continuation.resume(returning: outputURL)
                    } else {
                        continuation.resume(throwing: writer.error ?? NativeScreenRecordingError.writer("The recording could not be finalized."))
                    }
                }
            }
        }
    }

    private func appendVideo(_ sampleBuffer: CMSampleBuffer) {
        let presentationTime = sampleBuffer.presentationTimeStamp
        if sessionStartTime == nil {
            guard writer.startWriting() else {
                terminalError = writer.error ?? NativeScreenRecordingError.writer("The video writer could not start.")
                return
            }
            writer.startSession(atSourceTime: presentationTime)
            sessionStartTime = presentationTime
        }
        guard videoInput.isReadyForMoreMediaData else { return }
        if !videoInput.append(sampleBuffer) {
            terminalError = writer.error ?? NativeScreenRecordingError.writer("A video frame could not be written.")
        }
    }

    private func appendAudio(_ sampleBuffer: CMSampleBuffer) {
        guard let sessionStartTime,
              sampleBuffer.presentationTimeStamp >= sessionStartTime,
              let audioInput,
              audioInput.isReadyForMoreMediaData else { return }
        if !audioInput.append(sampleBuffer) {
            terminalError = writer.error ?? NativeScreenRecordingError.writer("A system audio frame could not be written.")
        }
    }
}
