
#ifndef __FLTUSER__
#define __FLTUSER__

#include <windows.h>
#include <winternl.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN64)
#define POINTER_ALIGNMENT DECLSPEC_ALIGN(8)
#else
#define POINTER_ALIGNMENT
#endif

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


typedef enum _DIRECTORY_NOTIFY_INFORMATION_CLASS {
	DirectoryNotifyInformation = 1,
	DirectoryNotifyExtendedInformation // 2
} DIRECTORY_NOTIFY_INFORMATION_CLASS, * PDIRECTORY_NOTIFY_INFORMATION_CLASS;

typedef enum _FSINFOCLASS {
	FileFsVolumeInformation = 1,
	FileFsLabelInformation,         // 2
	FileFsSizeInformation,          // 3
	FileFsDeviceInformation,        // 4
	FileFsAttributeInformation,     // 5
	FileFsControlInformation,       // 6
	FileFsFullSizeInformation,      // 7
	FileFsObjectIdInformation,      // 8
	FileFsDriverPathInformation,    // 9
	FileFsVolumeFlagsInformation,   // 10
	FileFsSectorSizeInformation,    // 11
	FileFsDataCopyInformation,      // 12
	FileFsMetadataSizeInformation,  // 13
	FileFsFullSizeInformationEx,    // 14
	FileFsMaximumInformation
} FS_INFORMATION_CLASS, * PFS_INFORMATION_CLASS;

typedef enum _DEVICE_RELATION_TYPE {
	BusRelations,
	EjectionRelations,
	PowerRelations,
	RemovalRelations,
	TargetDeviceRelation,
	SingleBusRelations,
	TransportRelations
} DEVICE_RELATION_TYPE, * PDEVICE_RELATION_TYPE;

typedef enum {
	BusQueryDeviceID = 0,       // <Enumerator>\<Enumerator-specific device id>
	BusQueryHardwareIDs = 1,    // Hardware ids
	BusQueryCompatibleIDs = 2,  // compatible device ids
	BusQueryInstanceID = 3,     // persistent id for this instance of the device
	BusQueryDeviceSerialNumber = 4,   // serial number for this device
	BusQueryContainerID = 5     // unique id of the device's physical container
} BUS_QUERY_ID_TYPE, * PBUS_QUERY_ID_TYPE;

typedef enum {
	DeviceTextDescription = 0,            // DeviceDesc property
	DeviceTextLocationInformation = 1     // DeviceLocation property
} DEVICE_TEXT_TYPE, * PDEVICE_TEXT_TYPE;


typedef enum _DEVICE_USAGE_NOTIFICATION_TYPE {
	DeviceUsageTypeUndefined,
	DeviceUsageTypePaging,
	DeviceUsageTypeHibernation,
	DeviceUsageTypeDumpFile,
	DeviceUsageTypeBoot,
	DeviceUsageTypePostDisplay,
	DeviceUsageTypeGuestAssigned
} DEVICE_USAGE_NOTIFICATION_TYPE;

typedef enum _FS_FILTER_SECTION_SYNC_TYPE {
	SyncTypeOther = 0,
	SyncTypeCreateSection
} FS_FILTER_SECTION_SYNC_TYPE, * PFS_FILTER_SECTION_SYNC_TYPE;

typedef struct _FS_FILTER_SECTION_SYNC_OUTPUT {
	ULONG StructureSize;
	ULONG SizeReturned;
	ULONG Flags;
	ULONG DesiredReadAlignment;
} FS_FILTER_SECTION_SYNC_OUTPUT, * PFS_FILTER_SECTION_SYNC_OUTPUT;

typedef union _FLT_PARAMETERS {

	//
	//  IRP_MJ_CREATE
	//

	struct {
		PVOID SecurityContext;

		//
		//  The low 24 bits contains CreateOptions flag values.
		//  The high 8 bits contains the CreateDisposition values.
		//

		ULONG Options;

		USHORT POINTER_ALIGNMENT FileAttributes;
		USHORT ShareAccess;
		ULONG POINTER_ALIGNMENT EaLength;

		PVOID EaBuffer;                 //Not in IO_STACK_LOCATION parameters list
		LARGE_INTEGER AllocationSize;   //Not in IO_STACK_LOCATION parameters list
	} Create;

	//
	//  IRP_MJ_CREATE_NAMED_PIPE
	//
	//  Notice that the fields in the following parameter structure must
	//  match those for the create structure other than the last longword.
	//  This is so that no distinctions need be made by the I/O system's
	//  parse routine other than for the last longword.
	//

	struct {
		PVOID SecurityContext;
		ULONG Options;
		USHORT POINTER_ALIGNMENT Reserved;
		USHORT ShareAccess;
		PVOID Parameters; // PNAMED_PIPE_CREATE_PARAMETERS
	} CreatePipe;

	//
	//  IRP_MJ_CREATE_MAILSLOT
	//
	//  Notice that the fields in the following parameter structure must
	//  match those for the create structure other than the last longword.
	//  This is so that no distinctions need be made by the I/O system's
	//  parse routine other than for the last longword.
	//

	struct {
		PVOID SecurityContext;
		ULONG Options;
		USHORT POINTER_ALIGNMENT Reserved;
		USHORT ShareAccess;
		PVOID Parameters; // PMAILSLOT_CREATE_PARAMETERS
	} CreateMailslot;

	//
	//  IRP_MJ_READ
	//

	struct {
		ULONG Length;                   //Length of transfer
		ULONG POINTER_ALIGNMENT Key;
		LARGE_INTEGER ByteOffset;       //Offset to read from

		PVOID ReadBuffer;       //Not in IO_STACK_LOCATION parameters list
		PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
	} Read;

	//
	//  IRP_MJ_WRITE
	//

	struct {
		ULONG Length;                   //Length of transfer
		ULONG POINTER_ALIGNMENT Key;
		LARGE_INTEGER ByteOffset;       //Offset to write to

		PVOID WriteBuffer;      //Not in IO_STACK_LOCATION parameters list
		PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
	} Write;

	//
	//  IRP_MJ_QUERY_INFORMATION
	//

	struct {
		ULONG Length;           //Length of buffer
		FILE_INFORMATION_CLASS POINTER_ALIGNMENT FileInformationClass; //Class of information to query

		PVOID InfoBuffer;       //Not in IO_STACK_LOCATION parameters list
	} QueryFileInformation;

	//
	//  IRP_MJ_SET_INFORMATION
	//

	struct {
		ULONG Length;
		FILE_INFORMATION_CLASS POINTER_ALIGNMENT FileInformationClass;
		PVOID ParentOfTarget;
		union {
			struct {
				BOOLEAN ReplaceIfExists;
				BOOLEAN AdvanceOnly;
			};
			ULONG ClusterCount;
			HANDLE DeleteHandle;
		};

		PVOID InfoBuffer;       //Not in IO_STACK_LOCATION parameters list
	} SetFileInformation;

	//
	//  IRP_MJ_QUERY_EA
	//

	struct {
		ULONG Length;
		PVOID EaList;
		ULONG EaListLength;
		ULONG POINTER_ALIGNMENT EaIndex;

		PVOID EaBuffer;         //Not in IO_STACK_LOCATION parameters list
		PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
	} QueryEa;

	//
	//  IRP_MJ_SET_EA
	//

	struct {
		ULONG Length;

		PVOID EaBuffer;         //Not in IO_STACK_LOCATION parameters list
		PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
	} SetEa;

	//
	//  IRP_MJ_QUERY_VOLUME_INFORMATION
	//

	struct {
		ULONG Length;
		FS_INFORMATION_CLASS POINTER_ALIGNMENT FsInformationClass;

		PVOID VolumeBuffer;     //Not in IO_STACK_LOCATION parameters list
	} QueryVolumeInformation;

	//
	//  IRP_MJ_SET_VOLUME_INFORMATION
	//

	struct {
		ULONG Length;
		FS_INFORMATION_CLASS POINTER_ALIGNMENT FsInformationClass;

		PVOID VolumeBuffer;     //Not in IO_STACK_LOCATION parameters list
	} SetVolumeInformation;

	//
	//  IRP_MJ_DIRECTORY_CONTROL
	//

	union {

		//
		//  IRP_MN_QUERY_DIRECTORY or IRP_MN_QUERY_OLE_DIRECTORY
		//

		struct {
			ULONG Length;
			PUNICODE_STRING FileName;
			FILE_INFORMATION_CLASS FileInformationClass;
			ULONG POINTER_ALIGNMENT FileIndex;

			PVOID DirectoryBuffer;  //Not in IO_STACK_LOCATION parameters list
			PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
		} QueryDirectory;

		//
		//  IRP_MN_NOTIFY_CHANGE_DIRECTORY
		//

		struct {
			ULONG Length;
			ULONG POINTER_ALIGNMENT CompletionFilter;

			//
			// These spares ensure that the offset of DirectoryBuffer is
			// exactly the same as that for QueryDirectory minor code. This
			// needs to be the same because filter manager code makes the assumption
			// they are the same
			//

			ULONG POINTER_ALIGNMENT Spare1;
			ULONG POINTER_ALIGNMENT Spare2;

			PVOID DirectoryBuffer;  //Not in IO_STACK_LOCATION parameters list
			PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
		} NotifyDirectory;

		//
		//  IRP_MN_NOTIFY_CHANGE_DIRECTORY_EX
		//

		struct {
			ULONG Length;
			ULONG POINTER_ALIGNMENT CompletionFilter;

			DIRECTORY_NOTIFY_INFORMATION_CLASS POINTER_ALIGNMENT DirectoryNotifyInformationClass;

			//
			// These spares ensure that the offset of DirectoryBuffer is
			// exactly the same as that for QueryDirectory minor code. This
			// needs to be the same because filter manager code makes the assumption
			// they are the same
			//

			ULONG POINTER_ALIGNMENT Spare2;

			PVOID DirectoryBuffer;  //Not in IO_STACK_LOCATION parameters list
			PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
		} NotifyDirectoryEx;

	} DirectoryControl;

	//
	//  IRP_MJ_FILE_SYSTEM_CONTROL
	//
	//  Note that the user's output buffer is stored in the UserBuffer field
	//  and the user's input buffer is stored in the SystemBuffer field.
	//

	union {

		//
		//  IRP_MN_VERIFY_VOLUME
		//

		struct {
			PVOID Vpb;
			PVOID DeviceObject;
		} VerifyVolume;

		//
		//  IRP_MN_KERNEL_CALL and IRP_MN_USER_FS_REQUEST
		//  The parameters are broken out into 3 separate unions based on the
		//  method of the FSCTL Drivers should use the method-appropriate
		//  union for accessing parameters
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT FsControlCode;
		} Common;

		//
		//  METHOD_NEITHER Fsctl parameters
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT FsControlCode;

			//
			//  Type3InputBuffer: name changed from IO_STACK_LOCATION parameters
			//  Note for this mothod, both input & output buffers are 'raw',
			//  i.e. unbuffered, and should be treated with caution ( either
			//  probed & captured before access, or use try-except to enclose
			//  access to the buffer)
			//

			PVOID InputBuffer;
			PVOID OutputBuffer;

			//
			//  Mdl address for the output buffer  (maybe NULL)
			//

			PVOID OutputMdlAddress;
		} Neither;

		//
		//  METHOD_BUFFERED Fsctl parameters
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT FsControlCode;

			//
			//  For method buffered, this buffer is used both for input and
			//  output
			//

			PVOID SystemBuffer;

		} Buffered;

		//
		//  METHOD_IN_DIRECT/METHOD_OUT_DIRECT Fsctl parameters
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT FsControlCode;

			//
			//  Note the input buffer is already captured & buffered here - so
			//  can be safely accessed from kernel mode.  The output buffer is
			//  locked down - so also safe to access, however the OutputBuffer
			//  pointer is the user virtual address, so if the driver wishes to
			//  access the buffer in a different process context than that of
			//  the original i/o - it will have to obtain the system address
			//  from the MDL
			//

			PVOID InputSystemBuffer;

			//
			//  User virtual address of output buffer
			//

			PVOID OutputBuffer;

			//
			//  Mdl address for the locked down output buffer (should be
			//  non-NULL)
			//

			PVOID OutputMdlAddress;
		} Direct;

	} FileSystemControl;

	//
	//  IRP_MJ_DEVICE_CONTROL or IRP_MJ_INTERNAL_DEVICE_CONTROL
	//

	union {

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT IoControlCode;
		} Common;

		//
		//  The parameters are broken out into 3 separate unions based on the
		//  method of the IOCTL.  Drivers should use the method-appropriate
		//  union for accessing parameters.
		//

		//
		//  METHOD_NEITHER Ioctl parameters for IRP path
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT IoControlCode;

			//
			//  Type3InputBuffer: name changed from IO_STACK_LOCATION parameters
			//  Note for this mothod, both input & output buffers are 'raw',
			//  i.e. unbuffered, and should be treated with caution ( either
			//  probed & captured before access, or use try-except to enclose
			//  access to the buffer)
			//

			PVOID InputBuffer;
			PVOID OutputBuffer;

			//
			//  Mdl address for the output buffer  (maybe NULL)
			//

			PVOID OutputMdlAddress;
		} Neither;

		//
		//  METHOD_BUFFERED Ioctl parameters for IRP path
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT IoControlCode;

			//
			//  For method buffered, this buffer is used both for input and
			//  output
			//

			PVOID SystemBuffer;

		} Buffered;

		//
		//  METHOD_IN_DIRECT/METHOD_OUT_DIRECT Ioctl parameters
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT IoControlCode;

			//
			//  Note the input buffer is already captured & buffered here - so
			//  can be safely accessed from kernel mode.  The output buffer is
			//  locked down - so also safe to access, however the OutputBuffer
			//  pointer is the user virtual address, so if the driver wishes to
			//  access the buffer in a different process context than that of
			//  the original i/o - it will have to obtain the system address
			//  from the MDL
			//

			PVOID InputSystemBuffer;

			//
			//  User virtual address of output buffer
			//

			PVOID OutputBuffer;

			//
			//  Mdl address for the locked down output buffer (should be non-NULL)
			//

			PVOID OutputMdlAddress;
		} Direct;

		//
		//  Regardless of method, if the CALLBACK_DATA represents a fast i/o
		//  device IOCTL, this structure must be used to access the parameters
		//

		struct {
			ULONG OutputBufferLength;
			ULONG POINTER_ALIGNMENT InputBufferLength;
			ULONG POINTER_ALIGNMENT IoControlCode;

			//
			//  Both buffers are 'raw', i.e. unbuffered
			//

			PVOID InputBuffer;
			PVOID OutputBuffer;

		} FastIo;

	} DeviceIoControl;

	//
	//  IRP_MJ_LOCK_CONTROL
	//

	struct {
		PLARGE_INTEGER Length;
		ULONG POINTER_ALIGNMENT Key;
		LARGE_INTEGER ByteOffset;

		PVOID ProcessId;        //  Only meaningful for FastIo locking operations.
		BOOLEAN FailImmediately;    //  Only meaningful for FastIo locking operations.
		BOOLEAN ExclusiveLock;      //  Only meaningful for FastIo locking operations.
	} LockControl;

	//
	//  IRP_MJ_QUERY_SECURITY
	//

	struct {
		SECURITY_INFORMATION SecurityInformation;
		ULONG POINTER_ALIGNMENT Length;

		PVOID SecurityBuffer;   //Not in IO_STACK_LOCATION parameters list
		PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
	} QuerySecurity;

	//
	//  IRP_MJ_SET_SECURITY
	//

	struct {
		SECURITY_INFORMATION SecurityInformation;
		PSECURITY_DESCRIPTOR SecurityDescriptor;
	} SetSecurity;

	//
	//  IRP_MJ_SYSTEM_CONTROL
	//

	struct {
		ULONG_PTR ProviderId;
		PVOID DataPath;
		ULONG BufferSize;
		PVOID Buffer;
	} WMI;

	//
	//  IRP_MJ_QUERY_QUOTA
	//

	struct {
		ULONG Length;
		PSID StartSid;
		PVOID SidList;
		ULONG SidListLength;

		PVOID QuotaBuffer;      //Not in IO_STACK_LOCATION parameters list
		PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
	} QueryQuota;

	//
	//  IRP_MJ_SET_QUOTA
	//

	struct {
		ULONG Length;

		PVOID QuotaBuffer;      //Not in IO_STACK_LOCATION parameters list
		PVOID MdlAddress;        //Mdl address for the buffer  (maybe NULL)
	} SetQuota;

	//
	//  IRP_MJ_PNP
	//

	union {

		//
		//  IRP_MN_START_DEVICE
		//

		struct {
			PVOID AllocatedResources;
			PVOID AllocatedResourcesTranslated;
		} StartDevice;

		//
		//  IRP_MN_QUERY_DEVICE_RELATIONS
		//

		struct {
			DEVICE_RELATION_TYPE Type;
		} QueryDeviceRelations;

		//
		//  IRP_MN_QUERY_INTERFACE
		//

		struct {
			CONST GUID* InterfaceType;
			USHORT Size;
			USHORT Version;
			PVOID Interface;
			PVOID InterfaceSpecificData;
		} QueryInterface;

		//
		//  IRP_MN_QUERY_CAPABILITIES
		//

		struct {
			PVOID Capabilities;
		} DeviceCapabilities;

		//
		//  IRP_MN_FILTER_RESOURCE_REQUIREMENTS
		//

		struct {
			PVOID IoResourceRequirementList;
		} FilterResourceRequirements;

		//
		//  IRP_MN_READ_CONFIG and IRP_MN_WRITE_CONFIG
		//

		struct {
			ULONG WhichSpace;
			PVOID Buffer;
			ULONG Offset;
			ULONG POINTER_ALIGNMENT Length;
		} ReadWriteConfig;

		//
		//  IRP_MN_SET_LOCK
		//

		struct {
			BOOLEAN Lock;
		} SetLock;

		//
		//  IRP_MN_QUERY_ID
		//

		struct {
			BUS_QUERY_ID_TYPE IdType;
		} QueryId;

		//
		//  IRP_MN_QUERY_DEVICE_TEXT
		//

		struct {
			DEVICE_TEXT_TYPE DeviceTextType;
			LCID POINTER_ALIGNMENT LocaleId;
		} QueryDeviceText;

		//
		//  IRP_MN_DEVICE_USAGE_NOTIFICATION
		//

		struct {
			BOOLEAN InPath;
			BOOLEAN Reserved[3];
			DEVICE_USAGE_NOTIFICATION_TYPE POINTER_ALIGNMENT Type;
		} UsageNotification;

	} Pnp;

	//
	//  ***** Start of Emulated IRP definitions
	//

	//
	//  IRP_MJ_ACQUIRE_FOR_SECTION_SYNCHRONIZATION
	//

	struct {
		FS_FILTER_SECTION_SYNC_TYPE SyncType;
		ULONG PageProtection;
		PFS_FILTER_SECTION_SYNC_OUTPUT OutputInformation;
	} AcquireForSectionSynchronization;

	//
	//  IRP_MJ_ACQUIRE_FOR_MOD_WRITE
	//

	struct {
		PLARGE_INTEGER EndingOffset;
		PVOID ResourceToRelease;
	} AcquireForModifiedPageWriter;

	//
	//  IRP_MJ_RELEASE_FOR_MOD_WRITE
	//

	struct {
		PVOID ResourceToRelease;
	} ReleaseForModifiedPageWriter;

	//
	// IRP_MJ_QUERY_OPEN
	//

	struct {
		PVOID Irp;
		PVOID FileInformation;
		PULONG Length;
		FILE_INFORMATION_CLASS FileInformationClass;
	} QueryOpen;


	//
	//  FAST_IO_CHECK_IF_POSSIBLE
	//

	struct {
		LARGE_INTEGER FileOffset;
		ULONG Length;
		ULONG POINTER_ALIGNMENT LockKey;
		BOOLEAN POINTER_ALIGNMENT CheckForReadOperation;
	} FastIoCheckIfPossible;

	//
	//  IRP_MJ_NETWORK_QUERY_OPEN
	//

	struct {
		PVOID Irp;
		PVOID NetworkInformation;
	} NetworkQueryOpen;

	//
	//  IRP_MJ_MDL_READ
	//

	struct {
		LARGE_INTEGER FileOffset;
		ULONG POINTER_ALIGNMENT Length;
		ULONG POINTER_ALIGNMENT Key;
		PVOID* MdlChain;
	} MdlRead;

	//
	//  IRP_MJ_MDL_READ_COMPLETE
	//

	struct {
		PVOID MdlChain;
	} MdlReadComplete;

	//
	//  IRP_MJ_PREPARE_MDL_WRITE
	//

	struct {
		LARGE_INTEGER FileOffset;
		ULONG POINTER_ALIGNMENT Length;
		ULONG POINTER_ALIGNMENT Key;
		PVOID* MdlChain;
	} PrepareMdlWrite;

	//
	//  IRP_MJ_MDL_WRITE_COMPLETE
	//

	struct {
		LARGE_INTEGER FileOffset;
		PVOID MdlChain;
	} MdlWriteComplete;

	//
	//  IRP_MJ_VOLUME_MOUNT
	//

	struct {
		ULONG DeviceType;
	} MountVolume;


	//
	// Others - driver-specific
	//

	struct {
		PVOID Argument1;
		PVOID Argument2;
		PVOID Argument3;
		PVOID Argument4;
		PVOID Argument5;
		LARGE_INTEGER Argument6;
	} Others;

} FLT_PARAMETERS, * PFLT_PARAMETERS;

#ifdef __cplusplus
}       //  Balance extern "C" above
#endif

#endif