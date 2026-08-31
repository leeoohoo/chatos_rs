#pragma once

#include <ntddk.h>
#include <wdf.h>
#include <fwpsk.h>
#include <fwpmk.h>
#include <ws2def.h>
#include <ws2ipdef.h>

#define NG_DEVICE_NAME L"\\Device\\ChatOSNetworkGuard"
#define NG_SYMBOLIC_NAME L"\\DosDevices\\ChatOSNetworkGuard"
#define NG_PROTOCOL_MAJOR 1
#define NG_HEALTH_MAGIC 0x31474E43u
#define NG_APPLY_MAGIC 0x32474E43u
#define NG_RESET_MAGIC 0x33474E43u
#define NG_REDIRECT_MAGIC 0x31524743u
#define NG_MAX_LEASES 128
#define NG_MAX_SID_BYTES SECURITY_MAX_SID_SIZE

#define IOCTL_NG_HEALTH CTL_CODE(FILE_DEVICE_NETWORK, 0x800, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA)
#define IOCTL_NG_APPLY_LEASE CTL_CODE(FILE_DEVICE_NETWORK, 0x801, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA)
#define IOCTL_NG_REMOVE_LEASE CTL_CODE(FILE_DEVICE_NETWORK, 0x802, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA)
#define IOCTL_NG_RESET_LEASES CTL_CODE(FILE_DEVICE_NETWORK, 0x803, METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA)

#pragma pack(push, 1)
typedef struct _NG_HEALTH_RESPONSE {
    UINT32 Magic;
    UINT16 ProtocolMajor;
    UINT16 ProtocolMinor;
    UINT32 Flags;
    UINT32 VersionMajor;
    UINT32 VersionMinor;
    UINT32 VersionPatch;
    UINT32 ActiveLeaseCount;
} NG_HEALTH_RESPONSE;

typedef struct _NG_APPLY_REQUEST {
    UINT32 Magic;
    UINT16 ProtocolMajor;
    UINT16 Reserved;
    GUID LeaseId;
    INT64 ExpiresAtUnixSeconds;
    UINT32 TargetProcessId;
    UINT16 HttpBrokerPort;
    UINT16 HttpsBrokerPort;
    UINT32 BrokerProcessId;
    UINT8 SidLength;
    UINT8 Reserved2[3];
    UINT8 Sid[ANYSIZE_ARRAY];
} NG_APPLY_REQUEST;

typedef struct _NG_REMOVE_REQUEST {
    UINT32 Magic;
    UINT16 ProtocolMajor;
    UINT16 Reserved;
    GUID LeaseId;
} NG_REMOVE_REQUEST;

typedef struct _NG_RESET_REQUEST {
    UINT32 Magic;
    UINT16 ProtocolMajor;
    UINT16 Reserved;
} NG_RESET_REQUEST;

typedef struct _NG_REDIRECT_CONTEXT {
    UINT32 Magic;
    UINT16 ProtocolMajor;
    UINT16 OriginalPort;
    GUID LeaseId;
} NG_REDIRECT_CONTEXT;
#pragma pack(pop)

typedef struct _NG_LEASE {
    BOOLEAN Active;
    UINT32 Generation;
    GUID LeaseId;
    INT64 ExpiresAtUnixSeconds;
    UINT32 TargetProcessId;
    UINT32 BrokerProcessId;
    UINT16 HttpBrokerPort;
    UINT16 HttpsBrokerPort;
    UINT8 SidLength;
    UINT8 Sid[NG_MAX_SID_BYTES];
    UINT64 FilterIds[12];
    UINT32 FilterCount;
} NG_LEASE;

NTSTATUS NgWfpInitialize(_In_ PDEVICE_OBJECT deviceObject);
VOID NgWfpUninitialize(VOID);
NTSTATUS NgWfpApplyLease(_In_reads_bytes_(requestBytes) const NG_APPLY_REQUEST* request, SIZE_T requestBytes);
NTSTATUS NgWfpRemoveLease(_In_ const GUID* leaseId);
NTSTATUS NgWfpResetLeases(VOID);
UINT32 NgWfpActiveLeaseCount(VOID);
BOOLEAN NgWfpSelfTest(VOID);
