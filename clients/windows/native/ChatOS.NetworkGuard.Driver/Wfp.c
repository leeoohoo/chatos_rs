#include <initguid.h>
#include "NetworkGuardProtocol.h"

DEFINE_GUID(NG_PROVIDER_KEY, 0x8b91e85a,0x80f9,0x4708,0xa7,0x76,0x98,0x04,0x29,0xe8,0xe5,0xc2);
DEFINE_GUID(NG_SUBLAYER_KEY, 0x0d29ba63,0xf1fb,0x4410,0xa2,0x15,0x25,0x98,0x6a,0x42,0xf8,0xbd);
DEFINE_GUID(NG_CALLOUT_V4_KEY, 0xbb3aa76c,0x3ef5,0x42a9,0x98,0x42,0xc3,0xe4,0x11,0x49,0xd5,0x31);
DEFINE_GUID(NG_CALLOUT_V6_KEY, 0x23344f12,0xe892,0x4f87,0xb4,0x58,0xa3,0x19,0x58,0xf0,0x45,0xb6);

static HANDLE gEngine;
static HANDLE gRedirectHandle;
static UINT32 gCalloutV4;
static UINT32 gCalloutV6;
static KSPIN_LOCK gLeaseLock;
static NG_LEASE gLeases[NG_MAX_LEASES];

static INT64 NgUnixSeconds(VOID)
{
    LARGE_INTEGER now;
    KeQuerySystemTimePrecise(&now);
    return (now.QuadPart - 116444736000000000LL) / 10000000LL;
}

static BOOLEAN NgGuidEqual(const GUID* left, const GUID* right)
{
    return RtlCompareMemory(left, right, sizeof(GUID)) == sizeof(GUID);
}

static VOID NTAPI NgClassify(
    const FWPS_INCOMING_VALUES0* values,
    const FWPS_INCOMING_METADATA_VALUES0* metadata,
    void* layerData,
    const void* classifyContext,
    const FWPS_FILTER0* filter,
    UINT64 flowContext,
    FWPS_CLASSIFY_OUT0* classifyOut)
{
    UNREFERENCED_PARAMETER(metadata);
    UNREFERENCED_PARAMETER(layerData);
    UNREFERENCED_PARAMETER(flowContext);
    if ((classifyOut->rights & FWPS_RIGHT_ACTION_WRITE) == 0) return;

    UINT32 slot = (UINT32)(filter->context & 0xffu);
    UINT32 generation = (UINT32)(filter->context >> 32);
    UINT16 originalPort = (UINT16)((filter->context >> 8) & 0xffffu);
    NG_LEASE snapshot;
    KIRQL irql;
    RtlZeroMemory(&snapshot, sizeof(snapshot));
    KeAcquireSpinLock(&gLeaseLock, &irql);
    if (slot < NG_MAX_LEASES) snapshot = gLeases[slot];
    KeReleaseSpinLock(&gLeaseLock, irql);

    if (!snapshot.Active || snapshot.Generation != generation ||
        snapshot.ExpiresAtUnixSeconds <= NgUnixSeconds()) {
        classifyOut->actionType = FWP_ACTION_BLOCK;
        classifyOut->rights &= ~FWPS_RIGHT_ACTION_WRITE;
        return;
    }

    UINT64 classifyHandle = 0;
    PVOID writable = NULL;
    NTSTATUS status = FwpsAcquireClassifyHandle0(classifyContext, 0, &classifyHandle);
    if (NT_SUCCESS(status)) {
        status = FwpsAcquireWritableLayerDataPointer0(
            classifyHandle, filter->filterId, 0, &writable, classifyOut);
    }
    if (!NT_SUCCESS(status) || writable == NULL) {
        if (classifyHandle != 0) FwpsReleaseClassifyHandle0(classifyHandle);
        classifyOut->actionType = FWP_ACTION_BLOCK;
        classifyOut->rights &= ~FWPS_RIGHT_ACTION_WRITE;
        return;
    }

    FWPS_CONNECT_REQUEST0* request = (FWPS_CONNECT_REQUEST0*)writable;
    NG_REDIRECT_CONTEXT redirectContext;
    RtlZeroMemory(&redirectContext, sizeof(redirectContext));
    redirectContext.Magic = NG_REDIRECT_MAGIC;
    redirectContext.ProtocolMajor = NG_PROTOCOL_MAJOR;
    redirectContext.OriginalPort = originalPort;
    redirectContext.LeaseId = snapshot.LeaseId;

    if (values->layerId == FWPS_LAYER_ALE_CONNECT_REDIRECT_V4) {
        SOCKADDR_IN* address = (SOCKADDR_IN*)request->remoteAddressAndPort;
        address->sin_family = AF_INET;
        address->sin_addr.S_un.S_addr = RtlUlongByteSwap(INADDR_LOOPBACK);
        address->sin_port = RtlUshortByteSwap(
            originalPort == 80 ? snapshot.HttpBrokerPort : snapshot.HttpsBrokerPort);
    } else {
        SOCKADDR_IN6* address = (SOCKADDR_IN6*)request->remoteAddressAndPort;
        address->sin6_family = AF_INET6;
        address->sin6_addr = in6addr_loopback;
        address->sin6_port = RtlUshortByteSwap(
            originalPort == 80 ? snapshot.HttpBrokerPort : snapshot.HttpsBrokerPort);
    }
    request->localRedirectTargetPID = snapshot.BrokerProcessId;
    request->localRedirectHandle = gRedirectHandle;
    request->localRedirectContext = &redirectContext;
    request->localRedirectContextSize = sizeof(redirectContext);
    FwpsApplyModifiedLayerData0(classifyHandle, writable, 0);
    FwpsReleaseClassifyHandle0(classifyHandle);
    classifyOut->actionType = FWP_ACTION_PERMIT;
}

static NTSTATUS NTAPI NgNotify(
    FWPS_CALLOUT_NOTIFY_TYPE notifyType,
    const GUID* filterKey,
    const FWPS_FILTER0* filter)
{
    UNREFERENCED_PARAMETER(notifyType);
    UNREFERENCED_PARAMETER(filterKey);
    UNREFERENCED_PARAMETER(filter);
    return STATUS_SUCCESS;
}

static VOID NTAPI NgFlowDelete(UINT16 layerId, UINT32 calloutId, UINT64 flowContext)
{
    UNREFERENCED_PARAMETER(layerId);
    UNREFERENCED_PARAMETER(calloutId);
    UNREFERENCED_PARAMETER(flowContext);
}

static NTSTATUS NgRegisterCallout(
    PDEVICE_OBJECT deviceObject,
    const GUID* key,
    const GUID* layer,
    UINT32* runtimeId)
{
    FWPS_CALLOUT0 runtime = {0};
    FWPM_CALLOUT0 management = {0};
    runtime.calloutKey = *key;
    runtime.classifyFn = NgClassify;
    runtime.notifyFn = NgNotify;
    runtime.flowDeleteFn = NgFlowDelete;
    NTSTATUS status = FwpsCalloutRegister0(deviceObject, &runtime, runtimeId);
    if (!NT_SUCCESS(status)) return status;
    management.calloutKey = *key;
    management.displayData.name = L"ChatOS NetworkGuard redirect";
    management.providerKey = (GUID*)&NG_PROVIDER_KEY;
    management.applicableLayer = *layer;
    status = FwpmCalloutAdd0(gEngine, &management, NULL, NULL);
    return status == FWP_E_ALREADY_EXISTS ? STATUS_SUCCESS : status;
}

NTSTATUS NgWfpInitialize(PDEVICE_OBJECT deviceObject)
{
    FWPM_SESSION0 session = {0};
    FWPM_PROVIDER0 provider = {0};
    FWPM_SUBLAYER0 sublayer = {0};
    KeInitializeSpinLock(&gLeaseLock);
    session.displayData.name = L"ChatOS NetworkGuard kernel session";
    session.flags = FWPM_SESSION_FLAG_DYNAMIC;
    NTSTATUS status = FwpmEngineOpen0(NULL, RPC_C_AUTHN_WINNT, NULL, &session, &gEngine);
    if (!NT_SUCCESS(status)) return status;
    provider.providerKey = NG_PROVIDER_KEY;
    provider.displayData.name = L"ChatOS NetworkGuard";
    status = FwpmProviderAdd0(gEngine, &provider, NULL);
    if (!NT_SUCCESS(status) && status != FWP_E_ALREADY_EXISTS) return status;
    sublayer.subLayerKey = NG_SUBLAYER_KEY;
    sublayer.displayData.name = L"ChatOS NetworkGuard";
    sublayer.providerKey = (GUID*)&NG_PROVIDER_KEY;
    sublayer.weight = 0xf000;
    status = FwpmSubLayerAdd0(gEngine, &sublayer, NULL);
    if (!NT_SUCCESS(status) && status != FWP_E_ALREADY_EXISTS) return status;
    status = FwpsRedirectHandleCreate0(&NG_PROVIDER_KEY, 0, &gRedirectHandle);
    if (!NT_SUCCESS(status)) return status;
    status = NgRegisterCallout(deviceObject, &NG_CALLOUT_V4_KEY,
        &FWPM_LAYER_ALE_CONNECT_REDIRECT_V4, &gCalloutV4);
    if (!NT_SUCCESS(status)) return status;
    return NgRegisterCallout(deviceObject, &NG_CALLOUT_V6_KEY,
        &FWPM_LAYER_ALE_CONNECT_REDIRECT_V6, &gCalloutV6);
}

static NTSTATUS NgAddFilter(
    NG_LEASE* lease,
    UINT32 slot,
    const GUID* layer,
    const GUID* callout,
    UINT16 port,
    FWP_ACTION_TYPE action,
    UINT64 weight)
{
    FWPM_FILTER0 filter = {0};
    FWPM_FILTER_CONDITION0 conditions[3] = {0};
    UINT64 context = ((UINT64)lease->Generation << 32) | ((UINT64)port << 8) | slot;
    conditions[0].fieldKey = FWPM_CONDITION_ALE_PACKAGE_ID;
    conditions[0].matchType = FWP_MATCH_EQUAL;
    conditions[0].conditionValue.type = FWP_SID;
    conditions[0].conditionValue.sid = (SID*)lease->Sid;
    conditions[1].fieldKey = FWPM_CONDITION_IP_PROTOCOL;
    conditions[1].matchType = FWP_MATCH_EQUAL;
    conditions[1].conditionValue.type = FWP_UINT8;
    conditions[1].conditionValue.uint8 = IPPROTO_TCP;
    conditions[2].fieldKey = FWPM_CONDITION_IP_REMOTE_PORT;
    conditions[2].matchType = FWP_MATCH_EQUAL;
    conditions[2].conditionValue.type = FWP_UINT16;
    conditions[2].conditionValue.uint16 = port;
    filter.displayData.name = L"ChatOS NetworkGuard lease filter";
    filter.providerKey = (GUID*)&NG_PROVIDER_KEY;
    filter.layerKey = *layer;
    filter.subLayerKey = NG_SUBLAYER_KEY;
    filter.weight.type = FWP_UINT64;
    filter.weight.uint64 = &weight;
    filter.numFilterConditions = port == 0 ? 2 : 3;
    filter.filterCondition = conditions;
    filter.action.type = action;
    if (action == FWP_ACTION_CALLOUT_TERMINATING) filter.action.calloutKey = *callout;
    filter.rawContext = context;
    return FwpmFilterAdd0(gEngine, &filter, NULL, &lease->FilterIds[lease->FilterCount++]);
}

static NTSTATUS NgAddAuthFilter(
    NG_LEASE* lease,
    const GUID* layer,
    UINT16 brokerPort,
    FWP_ACTION_TYPE action,
    UINT64 weight)
{
    FWPM_FILTER0 filter = {0};
    FWPM_FILTER_CONDITION0 conditions[3] = {0};
    conditions[0].fieldKey = FWPM_CONDITION_ALE_PACKAGE_ID;
    conditions[0].matchType = FWP_MATCH_EQUAL;
    conditions[0].conditionValue.type = FWP_SID;
    conditions[0].conditionValue.sid = (SID*)lease->Sid;
    conditions[1].fieldKey = FWPM_CONDITION_IP_PROTOCOL;
    conditions[1].matchType = FWP_MATCH_EQUAL;
    conditions[1].conditionValue.type = FWP_UINT8;
    conditions[1].conditionValue.uint8 = IPPROTO_TCP;
    conditions[2].fieldKey = FWPM_CONDITION_IP_REMOTE_PORT;
    conditions[2].matchType = FWP_MATCH_EQUAL;
    conditions[2].conditionValue.type = FWP_UINT16;
    conditions[2].conditionValue.uint16 = brokerPort;
    filter.displayData.name = action == FWP_ACTION_BLOCK
        ? L"ChatOS NetworkGuard default deny"
        : L"ChatOS NetworkGuard broker channel";
    filter.providerKey = (GUID*)&NG_PROVIDER_KEY;
    filter.layerKey = *layer;
    filter.subLayerKey = NG_SUBLAYER_KEY;
    filter.weight.type = FWP_UINT64;
    filter.weight.uint64 = &weight;
    filter.numFilterConditions = action == FWP_ACTION_BLOCK ? 1 : 3;
    filter.filterCondition = conditions;
    filter.action.type = action;
    return FwpmFilterAdd0(gEngine, &filter, NULL, &lease->FilterIds[lease->FilterCount++]);
}

static VOID NgDeleteFilters(NG_LEASE* lease)
{
    for (UINT32 index = 0; index < lease->FilterCount; index++) {
        if (lease->FilterIds[index] != 0) FwpmFilterDeleteById0(gEngine, lease->FilterIds[index]);
    }
    RtlZeroMemory(lease->FilterIds, sizeof(lease->FilterIds));
    lease->FilterCount = 0;
}

NTSTATUS NgWfpApplyLease(const NG_APPLY_REQUEST* request, SIZE_T requestBytes)
{
    if (request->Magic != NG_APPLY_MAGIC || request->ProtocolMajor != NG_PROTOCOL_MAJOR ||
        request->SidLength == 0 || request->SidLength > NG_MAX_SID_BYTES ||
        requestBytes != FIELD_OFFSET(NG_APPLY_REQUEST, Sid) + request->SidLength ||
        !RtlValidSid((PSID)request->Sid) || request->ExpiresAtUnixSeconds <= NgUnixSeconds() ||
        request->TargetProcessId == 0 || request->BrokerProcessId == 0 ||
        request->HttpBrokerPort == 0 || request->HttpsBrokerPort == 0) {
        return STATUS_INVALID_PARAMETER;
    }
    UINT32 slot = NG_MAX_LEASES;
    UINT32 oldSlot = NG_MAX_LEASES;
    for (UINT32 index = 0; index < NG_MAX_LEASES; index++) {
        if (gLeases[index].Active &&
            (NgGuidEqual(&gLeases[index].LeaseId, &request->LeaseId) ||
             (gLeases[index].SidLength == request->SidLength &&
              RtlEqualSid((PSID)gLeases[index].Sid, (PSID)request->Sid)))) {
            oldSlot = index;
        }
        if (slot == NG_MAX_LEASES && !gLeases[index].Active && index != oldSlot) slot = index;
    }
    if (slot == NG_MAX_LEASES) return STATUS_INSUFFICIENT_RESOURCES;
    NG_LEASE* lease = &gLeases[slot];
    KIRQL irql;
    KeAcquireSpinLock(&gLeaseLock, &irql);
    lease->Active = FALSE;
    RtlZeroMemory(lease->FilterIds, sizeof(lease->FilterIds));
    lease->FilterCount = 0;
    lease->Generation++;
    lease->LeaseId = request->LeaseId;
    lease->ExpiresAtUnixSeconds = request->ExpiresAtUnixSeconds;
    lease->TargetProcessId = request->TargetProcessId;
    lease->BrokerProcessId = request->BrokerProcessId;
    lease->HttpBrokerPort = request->HttpBrokerPort;
    lease->HttpsBrokerPort = request->HttpsBrokerPort;
    lease->SidLength = request->SidLength;
    RtlCopyMemory(lease->Sid, request->Sid, request->SidLength);
    KeReleaseSpinLock(&gLeaseLock, irql);

    NTSTATUS status = FwpmTransactionBegin0(gEngine, 0);
    if (!NT_SUCCESS(status)) return status;
    status = NgAddFilter(lease, slot, &FWPM_LAYER_ALE_CONNECT_REDIRECT_V4,
        &NG_CALLOUT_V4_KEY, 80, FWP_ACTION_CALLOUT_TERMINATING, 0xf100);
    if (NT_SUCCESS(status)) status = NgAddFilter(lease, slot, &FWPM_LAYER_ALE_CONNECT_REDIRECT_V4,
        &NG_CALLOUT_V4_KEY, 443, FWP_ACTION_CALLOUT_TERMINATING, 0xf100);
    if (NT_SUCCESS(status)) status = NgAddFilter(lease, slot, &FWPM_LAYER_ALE_CONNECT_REDIRECT_V6,
        &NG_CALLOUT_V6_KEY, 80, FWP_ACTION_CALLOUT_TERMINATING, 0xf100);
    if (NT_SUCCESS(status)) status = NgAddFilter(lease, slot, &FWPM_LAYER_ALE_CONNECT_REDIRECT_V6,
        &NG_CALLOUT_V6_KEY, 443, FWP_ACTION_CALLOUT_TERMINATING, 0xf100);
    if (NT_SUCCESS(status)) status = NgAddAuthFilter(lease, &FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        lease->HttpBrokerPort, FWP_ACTION_PERMIT, 0xf200);
    if (NT_SUCCESS(status)) status = NgAddAuthFilter(lease, &FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        lease->HttpsBrokerPort, FWP_ACTION_PERMIT, 0xf200);
    if (NT_SUCCESS(status)) status = NgAddAuthFilter(lease, &FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        lease->HttpBrokerPort, FWP_ACTION_PERMIT, 0xf200);
    if (NT_SUCCESS(status)) status = NgAddAuthFilter(lease, &FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        lease->HttpsBrokerPort, FWP_ACTION_PERMIT, 0xf200);
    if (NT_SUCCESS(status)) status = NgAddAuthFilter(lease, &FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        0, FWP_ACTION_BLOCK, 0xf000);
    if (NT_SUCCESS(status)) status = NgAddAuthFilter(lease, &FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        0, FWP_ACTION_BLOCK, 0xf000);
    if (NT_SUCCESS(status)) status = FwpmTransactionCommit0(gEngine);
    else FwpmTransactionAbort0(gEngine);
    if (!NT_SUCCESS(status)) { NgDeleteFilters(lease); return status; }
    KeAcquireSpinLock(&gLeaseLock, &irql);
    lease->Active = TRUE;
    if (oldSlot != NG_MAX_LEASES && oldSlot != slot) {
        gLeases[oldSlot].Active = FALSE;
    }
    KeReleaseSpinLock(&gLeaseLock, irql);
    if (oldSlot != NG_MAX_LEASES && oldSlot != slot) NgDeleteFilters(&gLeases[oldSlot]);
    return STATUS_SUCCESS;
}

NTSTATUS NgWfpRemoveLease(const GUID* leaseId)
{
    BOOLEAN removed = FALSE;
    for (UINT32 index = 0; index < NG_MAX_LEASES; index++) {
        NG_LEASE* lease = &gLeases[index];
        if (lease->Active && NgGuidEqual(&lease->LeaseId, leaseId)) {
            KIRQL irql;
            KeAcquireSpinLock(&gLeaseLock, &irql);
            lease->Active = FALSE;
            KeReleaseSpinLock(&gLeaseLock, irql);
            NgDeleteFilters(lease);
            removed = TRUE;
        }
    }
    return removed ? STATUS_SUCCESS : STATUS_NOT_FOUND;
}

NTSTATUS NgWfpResetLeases(VOID)
{
    for (UINT32 index = 0; index < NG_MAX_LEASES; index++) {
        NG_LEASE* lease = &gLeases[index];
        BOOLEAN active;
        KIRQL irql;
        KeAcquireSpinLock(&gLeaseLock, &irql);
        active = lease->Active;
        lease->Active = FALSE;
        KeReleaseSpinLock(&gLeaseLock, irql);
        if (active || lease->FilterCount != 0) NgDeleteFilters(lease);
    }
    return STATUS_SUCCESS;
}

UINT32 NgWfpActiveLeaseCount(VOID)
{
    UINT32 count = 0;
    for (UINT32 index = 0; index < NG_MAX_LEASES; index++) if (gLeases[index].Active) count++;
    return count;
}

BOOLEAN NgWfpSelfTest(VOID)
{
    return gEngine != NULL && gRedirectHandle != NULL && gCalloutV4 != 0 && gCalloutV6 != 0;
}

VOID NgWfpUninitialize(VOID)
{
    for (UINT32 index = 0; index < NG_MAX_LEASES; index++) NgDeleteFilters(&gLeases[index]);
    if (gCalloutV4 != 0) FwpsCalloutUnregisterById0(gCalloutV4);
    if (gCalloutV6 != 0) FwpsCalloutUnregisterById0(gCalloutV6);
    if (gRedirectHandle != NULL) FwpsRedirectHandleDestroy0(gRedirectHandle);
    if (gEngine != NULL) FwpmEngineClose0(gEngine);
    gEngine = NULL;
    gRedirectHandle = NULL;
}
