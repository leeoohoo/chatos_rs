#include "NetworkGuardProtocol.h"

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_UNLOAD NgDriverUnload;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL NgIoDeviceControl;

static VOID NgCompleteHealth(_In_ WDFREQUEST request)
{
    NG_HEALTH_RESPONSE* response = NULL;
    size_t bytes = 0;
    NTSTATUS status = WdfRequestRetrieveOutputBuffer(
        request, sizeof(NG_HEALTH_RESPONSE), (PVOID*)&response, &bytes);
    if (NT_SUCCESS(status)) {
        RtlZeroMemory(response, sizeof(*response));
        response->Magic = NG_HEALTH_MAGIC;
        response->ProtocolMajor = NG_PROTOCOL_MAJOR;
        response->Flags = 1u | (NgWfpSelfTest() ? 2u : 0u);
        response->VersionMajor = 1;
        response->VersionMinor = 0;
        response->VersionPatch = 0;
        response->ActiveLeaseCount = NgWfpActiveLeaseCount();
        WdfRequestCompleteWithInformation(request, STATUS_SUCCESS, sizeof(*response));
        return;
    }
    WdfRequestComplete(request, status);
}

VOID NgIoDeviceControl(
    WDFQUEUE queue,
    WDFREQUEST request,
    size_t outputBufferLength,
    size_t inputBufferLength,
    ULONG ioControlCode)
{
    UNREFERENCED_PARAMETER(queue);
    UNREFERENCED_PARAMETER(outputBufferLength);
    NTSTATUS status = STATUS_INVALID_DEVICE_REQUEST;
    PVOID buffer = NULL;
    size_t bytes = 0;

    if (ioControlCode == IOCTL_NG_HEALTH) {
        NgCompleteHealth(request);
        return;
    }
    status = WdfRequestRetrieveInputBuffer(request, 1, &buffer, &bytes);
    if (!NT_SUCCESS(status)) {
        WdfRequestComplete(request, status);
        return;
    }
    if (ioControlCode == IOCTL_NG_APPLY_LEASE &&
        inputBufferLength >= FIELD_OFFSET(NG_APPLY_REQUEST, Sid)) {
        status = NgWfpApplyLease((const NG_APPLY_REQUEST*)buffer, bytes);
    } else if (ioControlCode == IOCTL_NG_REMOVE_LEASE &&
        inputBufferLength == sizeof(NG_REMOVE_REQUEST)) {
        const NG_REMOVE_REQUEST* remove = (const NG_REMOVE_REQUEST*)buffer;
        status = remove->Magic == NG_APPLY_MAGIC && remove->ProtocolMajor == NG_PROTOCOL_MAJOR
            ? NgWfpRemoveLease(&remove->LeaseId)
            : STATUS_REVISION_MISMATCH;
    } else if (ioControlCode == IOCTL_NG_RESET_LEASES &&
        inputBufferLength == sizeof(NG_RESET_REQUEST)) {
        const NG_RESET_REQUEST* reset = (const NG_RESET_REQUEST*)buffer;
        status = reset->Magic == NG_RESET_MAGIC && reset->ProtocolMajor == NG_PROTOCOL_MAJOR
            ? NgWfpResetLeases()
            : STATUS_REVISION_MISMATCH;
    }
    WdfRequestComplete(request, status);
}

VOID NgDriverUnload(_In_ WDFDRIVER driver)
{
    UNREFERENCED_PARAMETER(driver);
    NgWfpUninitialize();
}

NTSTATUS DriverEntry(_In_ PDRIVER_OBJECT driverObject, _In_ PUNICODE_STRING registryPath)
{
    WDF_DRIVER_CONFIG driverConfig;
    WDFDRIVER driver;
    PWDFDEVICE_INIT deviceInit = NULL;
    WDFDEVICE device;
    WDF_IO_QUEUE_CONFIG queueConfig;
    UNICODE_STRING deviceName;
    UNICODE_STRING symbolicName;
    UNICODE_STRING security;
    NTSTATUS status;

    WDF_DRIVER_CONFIG_INIT(&driverConfig, WDF_NO_EVENT_CALLBACK);
    driverConfig.DriverInitFlags |= WdfDriverInitNonPnpDriver;
    driverConfig.EvtDriverUnload = NgDriverUnload;
    status = WdfDriverCreate(driverObject, registryPath, WDF_NO_OBJECT_ATTRIBUTES, &driverConfig, &driver);
    if (!NT_SUCCESS(status)) return status;

    deviceInit = WdfControlDeviceInitAllocate(driver, &SDDL_DEVOBJ_SYS_ALL_ADM_ALL);
    if (deviceInit == NULL) return STATUS_INSUFFICIENT_RESOURCES;
    RtlInitUnicodeString(&deviceName, NG_DEVICE_NAME);
    RtlInitUnicodeString(&symbolicName, NG_SYMBOLIC_NAME);
    RtlInitUnicodeString(&security, L"D:P(A;;GA;;;SY)(A;;GA;;;BA)");
    WdfDeviceInitSetDeviceType(deviceInit, FILE_DEVICE_NETWORK);
    WdfDeviceInitSetExclusive(deviceInit, TRUE);
    status = WdfDeviceInitAssignName(deviceInit, &deviceName);
    if (NT_SUCCESS(status)) status = WdfDeviceInitAssignSDDLString(deviceInit, &security);
    if (NT_SUCCESS(status)) status = WdfDeviceCreate(&deviceInit, WDF_NO_OBJECT_ATTRIBUTES, &device);
    if (!NT_SUCCESS(status)) {
        if (deviceInit != NULL) WdfDeviceInitFree(deviceInit);
        return status;
    }
    status = WdfDeviceCreateSymbolicLink(device, &symbolicName);
    if (!NT_SUCCESS(status)) return status;

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchSequential);
    queueConfig.EvtIoDeviceControl = NgIoDeviceControl;
    status = WdfIoQueueCreate(device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, WDF_NO_HANDLE);
    if (!NT_SUCCESS(status)) return status;
    status = NgWfpInitialize(WdfDeviceWdmGetDeviceObject(device));
    if (!NT_SUCCESS(status)) return status;
    WdfControlFinishInitializing(device);
    return STATUS_SUCCESS;
}
