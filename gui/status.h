#pragma once

#define IRP_MJ_CREATE                   0x00
#define IRP_MJ_CREATE_NAMED_PIPE        0x01
#define IRP_MJ_CLOSE                    0x02
#define IRP_MJ_READ                     0x03
#define IRP_MJ_WRITE                    0x04
#define IRP_MJ_QUERY_INFORMATION        0x05
#define IRP_MJ_SET_INFORMATION          0x06
#define IRP_MJ_QUERY_EA                 0x07
#define IRP_MJ_SET_EA                   0x08
#define IRP_MJ_FLUSH_BUFFERS            0x09
#define IRP_MJ_QUERY_VOLUME_INFORMATION 0x0a
#define IRP_MJ_SET_VOLUME_INFORMATION   0x0b
#define IRP_MJ_DIRECTORY_CONTROL        0x0c
#define IRP_MJ_FILE_SYSTEM_CONTROL      0x0d
#define IRP_MJ_DEVICE_CONTROL           0x0e
#define IRP_MJ_INTERNAL_DEVICE_CONTROL  0x0f
#define IRP_MJ_SHUTDOWN                 0x10
#define IRP_MJ_LOCK_CONTROL             0x11
#define IRP_MJ_CLEANUP                  0x12
#define IRP_MJ_CREATE_MAILSLOT          0x13
#define IRP_MJ_QUERY_SECURITY           0x14
#define IRP_MJ_SET_SECURITY             0x15
#define IRP_MJ_POWER                    0x16
#define IRP_MJ_SYSTEM_CONTROL           0x17
#define IRP_MJ_DEVICE_CHANGE            0x18
#define IRP_MJ_QUERY_QUOTA              0x19
#define IRP_MJ_SET_QUOTA                0x1a
#define IRP_MJ_PNP                      0x1b

#define IRP_MJ_ACQUIRE_FOR_SECTION_SYNCHRONIZATION   ((UCHAR)-1)
#define IRP_MJ_RELEASE_FOR_SECTION_SYNCHRONIZATION   ((UCHAR)-2)
#define IRP_MJ_ACQUIRE_FOR_MOD_WRITE                 ((UCHAR)-3)
#define IRP_MJ_RELEASE_FOR_MOD_WRITE                 ((UCHAR)-4)
#define IRP_MJ_ACQUIRE_FOR_CC_FLUSH                  ((UCHAR)-5)
#define IRP_MJ_RELEASE_FOR_CC_FLUSH                  ((UCHAR)-6)
#define IRP_MJ_QUERY_OPEN                            ((UCHAR)-7)

#define IRP_MJ_FAST_IO_CHECK_IF_POSSIBLE             ((UCHAR)-13)
#define IRP_MJ_NETWORK_QUERY_OPEN                    ((UCHAR)-14)
#define IRP_MJ_MDL_READ                              ((UCHAR)-15)
#define IRP_MJ_MDL_READ_COMPLETE                     ((UCHAR)-16)
#define IRP_MJ_PREPARE_MDL_WRITE                     ((UCHAR)-17)
#define IRP_MJ_MDL_WRITE_COMPLETE                    ((UCHAR)-18)
#define IRP_MJ_VOLUME_MOUNT                          ((UCHAR)-19)
#define IRP_MJ_VOLUME_DISMOUNT                       ((UCHAR)-20)

typedef enum _MAP_SOURCE_TYPE
{
	emArchiteture,
	emAuthId,
	emCategory,
	emCommandLine,
	emCompany,
	emCompletionTime,
	emDataTime,
	emDescription,
	emDetail,
	emDuration,
	emEventClass,
	emImagePath,
	emIntegrity,
	emOperation,
	emParentPid,
	emPath,
	emPID,
	emProcessName,
	emRelativeTime,
	emResult,
	emSequence,
	emSession,
	emTID,
	emTimeOfDay,
	emUser,
	emVersion,
	emVirtualize
}MAP_SOURCE_TYPE;

LPCTSTR
StatusGetDesc(
	NTSTATUS Status
);

LPCTSTR
GetClassStringMap(
	int Class
);

LPCTSTR 
GetOperatorStringMap(
	IN PLOG_ENTRY pEntry
);

CString
MapMonitorResult(
	MAP_SOURCE_TYPE SrcType,
	const CRefPtr<CEventView> pOptView);