/*++

Module Name:

    procmon.c

Abstract:

    This is the main module of the procmon miniFilter driver.

Environment:

    Kernel mode

--*/

#include <fltKernel.h>
#include <Wdmsec.h>
#include <dontuse.h>

#include "ntheader.h"
#include "globals.h"
#include "log.h"
#include "utils.h"
#include "process.h"
#include "file.h"
#include "reg.h"


#pragma prefast(disable:__WARNING_ENCODE_MEMBER_FUNCTION_POINTER, "Not valid for kernel mode drivers")

#define TAG_LOOKSIDE 'nmP'

#define SYSTEM_DRIVER_PATH L"\\SystemRoot\\System32\\Drivers\\"

typedef
NTSTATUS
(*FNMESSAGEPROCESSOR)(
	VOID
	);

PFLT_PORT gServerPort = NULL;
PDEVICE_OBJECT gProcmonDebugLoggerDeviceObject = NULL;
PDEVICE_OBJECT gDevProcmonExternalLogger = NULL;
PRKEVENT gProcmonExternalLoggerEnabledEvent = NULL;
HANDLE gEventHandle = NULL;
FNMESSAGEPROCESSOR gfnDataProcessor;

#define PTDBG_TRACE_ROUTINES            0x00000001
#define PTDBG_TRACE_OPERATION_STATUS    0x00000002

ULONG gTraceFlags = 0;


#define PT_DBG_PRINT( _dbgLevel, _string )          \
    (FlagOn(gTraceFlags,(_dbgLevel)) ?              \
        DbgPrint _string :                          \
        ((int)0))

#define INVALID_HANDLE_VALUE ((HANDLE) -1)
#define IsAddressInModule(_addr, _base, _size) (((ULONG_PTR)_addr > (ULONG_PTR)_base) && \
					(ULONG_PTR)_addr < (ULONG_PTR)((ULONG_PTR)_base + (ULONG)_size))

/*************************************************************************
    Prototypes
*************************************************************************/

EXTERN_C_START

DRIVER_INITIALIZE DriverEntry;
NTSTATUS
DriverEntry (
    _In_ PDRIVER_OBJECT DriverObject,
    _In_ PUNICODE_STRING RegistryPath
    );

NTSTATUS
ProcmonInstanceSetup (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_SETUP_FLAGS Flags,
    _In_ DEVICE_TYPE VolumeDeviceType,
    _In_ FLT_FILESYSTEM_TYPE VolumeFilesystemType
    );

VOID
ProcmonInstanceTeardownStart (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_TEARDOWN_FLAGS Flags
    );

VOID
ProcmonInstanceTeardownComplete (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_TEARDOWN_FLAGS Flags
    );

NTSTATUS
ProcmonUnload (
    _In_ FLT_FILTER_UNLOAD_FLAGS Flags
    );

NTSTATUS
ProcmonInstanceQueryTeardown (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_QUERY_TEARDOWN_FLAGS Flags
    );

FLT_PREOP_CALLBACK_STATUS
ProcmonPreOperation (
    _Inout_ PFLT_CALLBACK_DATA Data,
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _Flt_CompletionContext_Outptr_ PVOID *CompletionContext
    );

VOID
ProcmonOperationStatusCallback (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ PFLT_IO_PARAMETER_BLOCK ParameterSnapshot,
    _In_ NTSTATUS OperationStatus,
    _In_ PVOID RequesterContext
    );

FLT_POSTOP_CALLBACK_STATUS
ProcmonPostOperation (
    _Inout_ PFLT_CALLBACK_DATA Data,
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_opt_ PVOID CompletionContext,
    _In_ FLT_POST_OPERATION_FLAGS Flags
    );

FLT_PREOP_CALLBACK_STATUS
ProcmonPreOperationNoPostOperation (
    _Inout_ PFLT_CALLBACK_DATA Data,
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _Flt_CompletionContext_Outptr_ PVOID *CompletionContext
    );

BOOLEAN
procmonDoRequestOperationStatus(
    _In_ PFLT_CALLBACK_DATA Data
    );

EXTERN_C_END

VOID
ProcmonProcessExitOff(
	VOID
);

LONG
SetMessageProcessor(
	_In_ FNMESSAGEPROCESSOR Processor
);

VOID
ProcmonControlProcMonitor(
	_In_ ULONG Flags
);

NTSTATUS
ProcmonWriteToPbmFile(
	VOID
);

//
//  Assign text sections for each routine.
//

#ifdef ALLOC_PRAGMA
#pragma alloc_text(INIT, DriverEntry)
#pragma alloc_text(PAGE, ProcmonUnload)
#pragma alloc_text(PAGE, ProcmonInstanceQueryTeardown)
#pragma alloc_text(PAGE, ProcmonInstanceSetup)
#pragma alloc_text(PAGE, ProcmonInstanceTeardownStart)
#pragma alloc_text(PAGE, ProcmonInstanceTeardownComplete)
#endif

//
//  operation registration
//

const FLT_OPERATION_REGISTRATION Callbacks[] = {

#if 1
    { IRP_MJ_CREATE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_CREATE_NAMED_PIPE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_CLOSE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_READ,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_WRITE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_QUERY_INFORMATION,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_SET_INFORMATION,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_QUERY_EA,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_SET_EA,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_FLUSH_BUFFERS,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_QUERY_VOLUME_INFORMATION,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_SET_VOLUME_INFORMATION,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_DIRECTORY_CONTROL,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_FILE_SYSTEM_CONTROL,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_DEVICE_CONTROL,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_INTERNAL_DEVICE_CONTROL,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_SHUTDOWN,
      0,
	  ProcmonPreOperation,
      NULL },                               //post operations not supported

    { IRP_MJ_LOCK_CONTROL,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_CLEANUP,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_CREATE_MAILSLOT,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_QUERY_SECURITY,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_SET_SECURITY,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_QUERY_QUOTA,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_SET_QUOTA,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_PNP,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_ACQUIRE_FOR_SECTION_SYNCHRONIZATION,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_RELEASE_FOR_SECTION_SYNCHRONIZATION,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_ACQUIRE_FOR_MOD_WRITE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_RELEASE_FOR_MOD_WRITE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_ACQUIRE_FOR_CC_FLUSH,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_RELEASE_FOR_CC_FLUSH,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_FAST_IO_CHECK_IF_POSSIBLE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_NETWORK_QUERY_OPEN,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_MDL_READ,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_MDL_READ_COMPLETE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_PREPARE_MDL_WRITE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_MDL_WRITE_COMPLETE,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_VOLUME_MOUNT,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

    { IRP_MJ_VOLUME_DISMOUNT,
      0,
      ProcmonPreOperation,
      ProcmonPostOperation },

#endif

    { IRP_MJ_OPERATION_END }
};

//
//  This defines what we want to filter with FltMgr
//

CONST FLT_REGISTRATION FilterRegistration = {

    sizeof( FLT_REGISTRATION ),         //  Size
    FLT_REGISTRATION_VERSION,           //  Version
    0,                                  //  Flags

    NULL,                               //  Context
    Callbacks,                          //  Operation callbacks

    /*procmonUnload*/NULL,                           //  MiniFilterUnload

    ProcmonInstanceSetup,                    //  InstanceSetup
    ProcmonInstanceQueryTeardown,            //  InstanceQueryTeardown
    /*ProcmonInstanceTeardownStart*/NULL,            //  InstanceTeardownStart
    /*ProcmonInstanceTeardownComplete*/NULL,         //  InstanceTeardownComplete

    NULL,                               //  GenerateFileName
    NULL,                               //  GenerateDestinationFileName
    NULL                                //  NormalizeNameComponent

};



NTSTATUS
ProcmonInstanceSetup (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_SETUP_FLAGS Flags,
    _In_ DEVICE_TYPE VolumeDeviceType,
    _In_ FLT_FILESYSTEM_TYPE VolumeFilesystemType
    )
/*++

Routine Description:

    This routine is called whenever a new instance is created on a volume. This
    gives us a chance to decide if we need to attach to this volume or not.

    If this routine is not defined in the registration structure, automatic
    instances are always created.

Arguments:

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance and its associated volume.

    Flags - Flags describing the reason for this attach request.

Return Value:

    STATUS_SUCCESS - attach
    STATUS_FLT_DO_NOT_ATTACH - do not attach

--*/
{
    UNREFERENCED_PARAMETER( FltObjects );
    UNREFERENCED_PARAMETER( Flags );
    UNREFERENCED_PARAMETER( VolumeDeviceType );
    UNREFERENCED_PARAMETER( VolumeFilesystemType );

    PAGED_CODE();

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!procmonInstanceSetup: Entered\n") );

    return STATUS_SUCCESS;
}


NTSTATUS
ProcmonInstanceQueryTeardown (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_QUERY_TEARDOWN_FLAGS Flags
    )
/*++

Routine Description:

    This is called when an instance is being manually deleted by a
    call to FltDetachVolume or FilterDetach thereby giving us a
    chance to fail that detach request.

    If this routine is not defined in the registration structure, explicit
    detach requests via FltDetachVolume or FilterDetach will always be
    failed.

Arguments:

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance and its associated volume.

    Flags - Indicating where this detach request came from.

Return Value:

    Returns the status of this operation.

--*/
{
    UNREFERENCED_PARAMETER( FltObjects );
    UNREFERENCED_PARAMETER( Flags );

    PAGED_CODE();

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!procmonInstanceQueryTeardown: Entered\n") );

    return STATUS_SUCCESS;
}


VOID
ProcmonInstanceTeardownStart (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_TEARDOWN_FLAGS Flags
    )
/*++

Routine Description:

    This routine is called at the start of instance teardown.

Arguments:

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance and its associated volume.

    Flags - Reason why this instance is being deleted.

Return Value:

    None.

--*/
{
    UNREFERENCED_PARAMETER( FltObjects );
    UNREFERENCED_PARAMETER( Flags );

    PAGED_CODE();

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!procmonInstanceTeardownStart: Entered\n") );
}


VOID
ProcmonInstanceTeardownComplete (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ FLT_INSTANCE_TEARDOWN_FLAGS Flags
    )
/*++

Routine Description:

    This routine is called at the end of instance teardown.

Arguments:

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance and its associated volume.

    Flags - Reason why this instance is being deleted.

Return Value:

    None.

--*/
{
    UNREFERENCED_PARAMETER( FltObjects );
    UNREFERENCED_PARAMETER( Flags );

    PAGED_CODE();

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!procmonInstanceTeardownComplete: Entered\n") );
}

PFLT_PORT gClientPort;

NTSTATUS 
FltOurSendMessage(
	_In_ PVOID SenderBuffer, 
	_In_ ULONG SenderBufferLength
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	return FltSendMessage(gFilterHandle, &gClientPort, SenderBuffer, 
		SenderBufferLength, NULL, NULL, NULL);
}

NTSTATUS 
ProcmonWriteToFltMessage(
	VOID
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	return ProcessLogDataWithCallback(FltOurSendMessage);
}

NTSTATUS
FLTAPI 
FltConnectNotify(
	_In_ PFLT_PORT ClientPort,
	_In_opt_ PVOID ServerPortCookie,
	_In_reads_bytes_opt_(SizeOfContext) PVOID ConnectionContext,
	_In_ ULONG SizeOfContext,
	_Outptr_result_maybenull_ PVOID *ConnectionPortCookie
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	ULONG Flag;

	UNREFERENCED_PARAMETER(ServerPortCookie);

	if (SizeOfContext != 4)
		return STATUS_INVALID_PARAMETER;

	Flag = *(PULONG)ConnectionContext;
	if (Flag){
		if (Flag == 1){
			
			//
			// 这个port使用来用环三来处理进程路径.
			// 如果内核不能获取进程的路径,则通过这个Port来通知环三
			//
			
			gClientProcessPathPort = ClientPort;
			*ConnectionPortCookie = &gClientProcessPathPort;
		}
		Status = STATUS_SUCCESS;
	}else if (gCurrentProcess){
		Status = STATUS_TOO_MANY_OPENED_FILES;
	}else{
		ProcmonProcessExitOff();
		SetMessageProcessor(ProcmonWriteToFltMessage);
		gCurrentProcess = IoGetCurrentProcess();
		gProcessId = PsGetCurrentProcessId();
		gClientPort = ClientPort;
		*ConnectionPortCookie = &gClientPort;
		Status = STATUS_SUCCESS;
	}
	
	return Status;
}

VOID
FLTAPI 
FltDisconnectNotify(
	_In_opt_ PVOID ConnectionCookie
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	if (ConnectionCookie == &gClientPort){
		gCurrentProcess = NULL;
		ProcmonControlProcMonitor(0);
		EnableFileMonitor(FALSE);
	}
	FltCloseClientPort(gFilterHandle, ConnectionCookie);
}

NTSTATUS
FLTAPI 
FltMessageNotify(
	_In_opt_ PVOID PortCookie,
	_In_reads_bytes_opt_(InputBufferLength) PVOID InputBuffer,
	_In_ ULONG InputBufferLength,
	_Out_writes_bytes_to_opt_(OutputBufferLength, *ReturnOutputBufferLength) PVOID OutputBuffer,
	_In_ ULONG OutputBufferLength,
	_Out_ PULONG ReturnOutputBufferLength
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{

	PFLTMSG_CONTROL_HEAD Header = (PFLTMSG_CONTROL_HEAD)InputBuffer;

	UNREFERENCED_PARAMETER(ReturnOutputBufferLength);
	UNREFERENCED_PARAMETER(OutputBufferLength);
	UNREFERENCED_PARAMETER(OutputBuffer);
	UNREFERENCED_PARAMETER(PortCookie);

	if (InputBufferLength < sizeof(FLTMSG_CONTROL_HEAD))
		return STATUS_INVALID_PARAMETER;
	if (Header->CtlCode){
		
		//
		// Enable or disable Thread profiling
		//
		
		if (Header->CtlCode == 1){
			if (InputBufferLength >= sizeof(FLTMSG_CONTROL_THREADPROFILING)){
				PFLTMSG_CONTROL_THREADPROFILING pThreadProfiling = (PFLTMSG_CONTROL_THREADPROFILING)InputBuffer;
				ProcmonEnableThreadProfiling(pThreadProfiling->ThreadProfile);
				return 0;
			}
			return STATUS_INVALID_PARAMETER;
		}
	}else{

		PFLTMSG_CONTROL_FLAGS pFlags = (PFLTMSG_CONTROL_FLAGS)InputBuffer;
		if (InputBufferLength < sizeof(FLTMSG_CONTROL_FLAGS))
			return STATUS_INVALID_PARAMETER;
		
		//
		// Control the procmon on/off
		//
		
		ProcmonControlProcMonitor(pFlags->Flags);
	}
	return STATUS_SUCCESS;
}

BOOLEAN
PromonCleanupDevice(
	VOID
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	UNICODE_STRING strDeviceSymbName;

	RtlInitUnicodeString(&strDeviceSymbName, PROCMON_DEBUGLOGGER_SYMBOL_NAME);
	IoDeleteSymbolicLink(&strDeviceSymbName);
	
	//
	// Clean up debug logger device
	//
	
	if (gProcmonDebugLoggerDeviceObject)
		IoDeleteDevice(gProcmonDebugLoggerDeviceObject);

	//
	// Clean up external logger device
	//

	if (gDevProcmonExternalLogger)
		IoDeleteDevice(gDevProcmonExternalLogger);
	
	//
	// Clean up external logger event object
	//
	
	if (gProcmonExternalLoggerEnabledEvent){
		KeClearEvent(gProcmonExternalLoggerEnabledEvent);
		ZwClose(gEventHandle);
	}
	return 1;
}

NTSTATUS
ProcmonStartFileFilter(
	_In_ PDRIVER_OBJECT DriverObject
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	
	NTSTATUS Status;
	OBJECT_ATTRIBUTES ObjectAttributes;
	UNICODE_STRING strObjectName;
	PSECURITY_DESCRIPTOR pSecurityDescriptor;

	//
	// Init spinlock for file name information list
	//
	
	KeInitializeSpinLock(&gFileNameInfoListSpinLock);
	
	//
	// Register with FltMgr to tell it our callback routines
	//
	
	Status = FltRegisterFilter(DriverObject, &FilterRegistration, &gFilterHandle);
	FLT_ASSERT(NT_SUCCESS(Status));

	if (NT_SUCCESS(Status)){
		
		//
		// Build security descriptor for server port
		//
		
		Status = FltBuildDefaultSecurityDescriptor(&pSecurityDescriptor, FLT_PORT_ALL_ACCESS);
		if (NT_SUCCESS(Status)){

			//
			// Set server port name and security descriptor
			//

			RtlInitUnicodeString(&strObjectName, PROCMON_PORTNAME);
			InitializeObjectAttributes(&ObjectAttributes, &strObjectName, 576, NULL, pSecurityDescriptor);
			
			//
			// Create communication port, we should use this port to communicate with user-mode app
			//
			
			FltCreateCommunicationPort(
				gFilterHandle,
				&gServerPort,
				&ObjectAttributes,
				NULL,
				FltConnectNotify,
				FltDisconnectNotify,
				FltMessageNotify,
				2);
			
			//
			// security descriptor is not useful here just release it
			//
			
			FltFreeSecurityDescriptor(pSecurityDescriptor);
			
			//
			// Start the filter
			//
			
			Status = FltStartFiltering(gFilterHandle);
		}
	}

	//
	// If somewhere failed clean up
	//
	
	if (!NT_SUCCESS(Status)){
		
		//
		// Close all device we create
		//
		
		PromonCleanupDevice();
		
		//
		// clean server port
		//
		
		if (gServerPort)
			FltCloseCommunicationPort(gServerPort);
		
		//
		// clean filter handle
		//
		
		if (gFilterHandle)
			FltUnregisterFilter(gFilterHandle);
	}
	return Status;
}


VOID
ProcessDataDpcRoutine(
	_In_ struct _KDPC *Dpc,
	_In_opt_ PVOID DeferredContext,
	_In_opt_ PVOID SystemArgument1,
	_In_opt_ PVOID SystemArgument2
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	UNREFERENCED_PARAMETER(Dpc);
	UNREFERENCED_PARAMETER(DeferredContext);
	UNREFERENCED_PARAMETER(SystemArgument1);
	UNREFERENCED_PARAMETER(SystemArgument2);

	KeSetEvent(&gEventProcessData, LOW_PRIORITY, FALSE);
}


VOID
ProcessDataThread(
	_In_ PVOID StartContext
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	UNREFERENCED_PARAMETER(StartContext);
	while (!KeWaitForSingleObject(&gEventProcessData, Executive, KernelMode, FALSE, NULL))
	{
		
		//
		// Process data
		//
		
		while (NT_SUCCESS(gfnDataProcessor()))
			;
	}
}

NTSTATUS
ProcmonInitialize(
	VOID
)
/*++

Routine Description:

    This routine use to initialize all the locks and lists.

Arguments:

	 None

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	ULONG NeedLength = 1000;
	PVOID pBuffer = NULL;
	OBJECT_ATTRIBUTES ObjectAttributes;
	HANDLE hThread;

	KeInitializeEvent(&gEventProcessData, SynchronizationEvent, 0);
	KeInitializeTimer(&gTimerProcessLogData);
	KeInitializeDpc(&gDpcProcessData, ProcessDataDpcRoutine, NULL);
	InitializeListHead(&gLogListHead);

	ExInitializeNPagedLookasideList(&gNPagedLooksideListLogBuffer, NULL, NULL, 0, 
		MAX_PROCMON_MESSAGE_LEN + sizeof(LOG_BUFFER),
		TAG_LOOKSIDE, 0);
	ExInitializeFastMutex(&gMutexLogList);
	InitializeListHead(&gThreadInfoList);
	ExInitializeNPagedLookasideList(&gNPagedLooksideListThreadInfo, NULL, NULL, 0, sizeof(THREADINFO_LIST), TAG_LOOKSIDE, 0);
	ExInitializeFastMutex(&gThreadInfoMutex);
	
	do 
	{
		if (pBuffer){
			ExFreePoolWithTag(pBuffer, 0);
		}

		pBuffer = ProcmonAllocatePoolWithTag(PagedPool, NeedLength, 'G');
		
		//
		// try to query 
		//
		
		Status = ZwQuerySystemInformation(SystemModuleInformation, pBuffer, NeedLength, &NeedLength);
		if (NT_SUCCESS(Status) || Status != STATUS_INFO_LENGTH_MISMATCH){
			break;
		}
		
		//
		// add NeedLength and continue
		//
		
		NeedLength += 1000;


	} while (TRUE);

	if (NT_SUCCESS(Status)){
		PRTL_PROCESS_MODULES pModuleInfo = (PRTL_PROCESS_MODULES)pBuffer;
		for (int i = 0; i < (int)pModuleInfo->NumberOfModules; i++)
		{
			PRTL_PROCESS_MODULE_INFORMATION pModule = &pModuleInfo->Modules[i];
			if (IsAddressInModule(ProcmonInitialize, pModule->ImageBase, pModule->ImageSize)){
				gSelfImageBase = pModule->ImageBase;
				gSelfImageSize = pModule->ImageSize;
				break;
			}
		}
		
		//
		// do not forget release the buffer
		//
		
		ExFreePoolWithTag(pBuffer, 0);
	}
	
	//
	// Create a new thread for data process
	//
	
	InitializeObjectAttributes(&ObjectAttributes, NULL, 512, NULL, NULL);
	
	gWriteFileFrqs = 100000000;

	PsCreateSystemThread(
		&hThread,
		0x1F03FF,
		&ObjectAttributes,
		NULL,
		NULL,
		ProcessDataThread,
		NULL);
	return STATUS_SUCCESS;
}


KTIMER gTimerRuntimes;
KDPC gDpcRuntimes;
KEVENT gEventFileWriteFiled;


typedef
NTSTATUS
(NTAPI *FNMESSAGEPROCESSOR)(
	VOID
	);

FNMESSAGEPROCESSOR gfnDataProcessor;

LONG 
SetMessageProcessor(
	_In_ FNMESSAGEPROCESSOR Processor
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	gfnDataProcessor = Processor;
	return KeSetEvent(&gEventProcessData, 0, 0);
}


VOID 
ProcmonRuntimeDpcWorkRoutine(
	_In_ PVOID Parameter
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	PWORK_QUEUE_ITEM pWorkItem = (PWORK_QUEUE_ITEM)Parameter;
	if (gFlags){
		
		//
		// Collect the process data an turn off the monitor
		//
		
		ProcmonCollectProcessAndSystemPerformanceData();
		SetMessageProcessor(ProcmonWriteToPbmFile);
		KeWaitForSingleObject(&gEventFileWriteFiled, 0, 0, 0, NULL);
		ProcmonControlProcMonitor(0);
	}
	ExFreePoolWithTag(pWorkItem, 0);
}

VOID
ProcmonRuntimeDpcRoutine(
	_In_     struct _KDPC *Dpc,
	_In_opt_ PVOID        DeferredContext,
	_In_opt_ PVOID        SystemArgument1,
	_In_opt_ PVOID        SystemArgument2
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	PWORK_QUEUE_ITEM pWorkItem;

	UNREFERENCED_PARAMETER(Dpc);
	UNREFERENCED_PARAMETER(DeferredContext);
	UNREFERENCED_PARAMETER(SystemArgument1);
	UNREFERENCED_PARAMETER(SystemArgument2);

	pWorkItem = ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(WORK_QUEUE_ITEM), 'M');
	if (pWorkItem) {
		pWorkItem->Parameter = pWorkItem;
		pWorkItem->WorkerRoutine = ProcmonRuntimeDpcWorkRoutine;
		pWorkItem->List.Flink = NULL;
		ExQueueWorkItem(pWorkItem, DelayedWorkQueue);
	}
}

HANDLE ghPMBFile = INVALID_HANDLE_VALUE;

NTSTATUS 
ProcmonCreateProcmonPmbFile(
	VOID
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	OBJECT_ATTRIBUTES ObjectAttributes;
	UNICODE_STRING UniStrPMBFileName;
	IO_STATUS_BLOCK IoStatusBlock;

	if (ghPMBFile != INVALID_HANDLE_VALUE)
		return STATUS_SUCCESS;

	RtlInitUnicodeString(&UniStrPMBFileName, PROCMON_DEFAULT_LOGFILE);
	InitializeObjectAttributes(&ObjectAttributes, &UniStrPMBFileName, 0x40, NULL, NULL);
	return ZwCreateFile(&ghPMBFile, 0x100002, &ObjectAttributes, &IoStatusBlock, 
		NULL, 0x80, 1u, 5, 0x20, NULL, 0);
}

NTSTATUS
ProcmonWriteMessageToFile(
	_In_ PVOID SenderBuffer,
	_In_ ULONG SenderBufferLength
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	IO_STATUS_BLOCK IoStatusBlock;
	LARGE_INTEGER FileInformation;

	UNREFERENCED_PARAMETER(SenderBufferLength);

	Status = ZwWriteFile(ghPMBFile, NULL, NULL, NULL, &IoStatusBlock, 
		(PVOID)((ULONG_PTR)SenderBuffer + sizeof(ULONG)), 
		*(PULONG)SenderBuffer, NULL, NULL);
	if (IoStatusBlock.Status == STATUS_DISK_FULL){
		FileInformation.QuadPart = 0;
		ZwSetInformationFile(ghPMBFile, &IoStatusBlock, &FileInformation, sizeof(FileInformation), FileEndOfFileInformation);
		Status = ProcessLogDataWithCallback(ProcmonWriteMessageToFile);
		if (!NT_SUCCESS(Status)){
			if (ghPMBFile != INVALID_HANDLE_VALUE){
				ZwClose(ghPMBFile);
				ghPMBFile = INVALID_HANDLE_VALUE;
				Status = KeSetEvent(&gEventFileWriteFiled, 0, 0);
			}
			gWriteFileFrqs = 100000000;
		}
	}
	return Status;
}

NTSTATUS 
ProcmonCreatePbmFiles(
	VOID
)
/*++

Routine Description:

    This function will open the pbm log file at default path "\\SystemRoot\\Procmon.pmb"
	And the write the log data which save in list to pbm log file.

Arguments:

	 -None

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status = ProcmonCreateProcmonPmbFile();
	if (NT_SUCCESS(Status)) {
		
		//
		// We write the data to pmb file
		//
		
		return ProcessLogDataWithCallback(ProcmonWriteMessageToFile);
	}
	ProcmonCleanupWriteState();
	return STATUS_END_OF_FILE;
}

NTSTATUS
ProcmonWriteToPbmFile(
	VOID
)
/*++

Routine Description:

    这个函数用来将链表中保存的日志数据保存到文件中.

Arguments:

	 None

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;

	Status = ProcessLogDataWithCallback(ProcmonWriteMessageToFile);
	if (!NT_SUCCESS(Status)){
		if (ghPMBFile != INVALID_HANDLE_VALUE){
			ZwClose(ghPMBFile);
			ghPMBFile = INVALID_HANDLE_VALUE;
			KeSetEvent(&gEventFileWriteFiled, 0, 0);
		}
		gWriteFileFrqs = 100000000;
		Status = STATUS_END_OF_FILE;
	}
	return Status;
}

BOOLEAN 
ProcmonGetProcessParameter(
	_In_ PUNICODE_STRING pUniStrRegPath, 
	_Out_ PLONG pThreadProfiling, 
	_Out_ PLONG pRuntimeSeconds
)
/*++

Routine Description:

    这个函数会从注册表中读取驱动的参数.这里主要有ThreadProfiling 和 RuntimeSeconds.
	且都是REG_DWORD类型.

Arguments:

	 pUniStrRegPath - 驱动注册表路径,从DriverEntry传入.
	 pThreadProfiling - 用于保存获取的ThreadProfiling值
	 pRuntimeSeconds - 用于保存获取的RuntimeSeconds值

Return Value:

    Success - TRUE , failed - FALSE

--*/
{
	NTSTATUS Status;
	ULONG ResultLength;
	OBJECT_ATTRIBUTES ObjectAttributes;
	UNICODE_STRING ValueName;
	UNICODE_STRING UnistrKeyName;
	UNICODE_STRING UnistrParameters;
	HANDLE KeyHandle;
	HANDLE Handle;
	ULONG Value = 3;
	UCHAR KeyInfo[sizeof(KEY_VALUE_PARTIAL_INFORMATION) + sizeof(ULONG)];
	PKEY_VALUE_PARTIAL_INFORMATION pKeyInfo = (PKEY_VALUE_PARTIAL_INFORMATION)KeyInfo;


	InitializeObjectAttributes(&ObjectAttributes, pUniStrRegPath, 0x40, NULL, NULL);

	Status = ZwOpenKey(&KeyHandle, 0x2001Fu, &ObjectAttributes);
	if (NT_SUCCESS(Status)){
		RtlInitUnicodeString(&UnistrKeyName, L"Start");
		Status = ZwQueryValueKey(KeyHandle, &UnistrKeyName, KeyValuePartialInformation, pKeyInfo, sizeof(KeyInfo), &ResultLength);
		if (NT_SUCCESS(Status)){
			Value = *(PLONG)pKeyInfo->Data;
			if (!Value){

				ULONG DefaultStart = 3;

				RtlInitUnicodeString(&UnistrParameters, L"Parameters");
				InitializeObjectAttributes(&ObjectAttributes, &UnistrParameters, 0x40, KeyHandle, NULL);
				Status = ZwOpenKey(&Handle, 0x2001Fu, &ObjectAttributes);
				if (NT_SUCCESS(Status)){
					RtlInitUnicodeString(&ValueName, L"ThreadProfiling");
					Status = ZwQueryValueKey(Handle, &ValueName, KeyValuePartialInformation, 
						pKeyInfo, sizeof(KeyInfo), &ResultLength);
					if (NT_SUCCESS(Status))
						*pThreadProfiling = *(PLONG)pKeyInfo->Data;
					RtlInitUnicodeString(&ValueName, L"RuntimeSeconds");
					Status = ZwQueryValueKey(Handle, &ValueName, KeyValuePartialInformation, pKeyInfo, 
						sizeof(KeyInfo), &ResultLength);
					if (NT_SUCCESS(Status))
						*pRuntimeSeconds = *(PLONG)pKeyInfo->Data;
					ZwClose(Handle);
				}
				
				ZwSetValueKey(KeyHandle, &UnistrKeyName, 0, REG_DWORD, &DefaultStart, sizeof(ULONG));
				ZwFlushKey(KeyHandle);
			}
		}
		ZwClose(KeyHandle);
	}
	return Value == 0;
}


VOID
ProcmonEnumLoadedModule(
	VOID
)
/*++

Routine Description:

    这个函数用来枚举系统驱动模块,并记录之,这里需要注意的是Procmon始终将自己驱动的信息放在第一个.

Arguments:

	 None 

Return Value:

     None

--*/
{
	NTSTATUS Status;
	UNICODE_STRING UnistrDriversPath;
	PVOID pBuffer = NULL;
	ULONG NeedLength = 1000;
	ULONG Round = 0;
	PRTL_PROCESS_MODULES pSysModuleInfo;
	IMAGE_INFO ImageInfo = {0};

	RtlInitUnicodeString(&UnistrDriversPath, SYSTEM_DRIVER_PATH);

	do
	{
		if (pBuffer) {
			ExFreePoolWithTag(pBuffer, 0);
		}

		pBuffer = ProcmonAllocatePoolWithTag(PagedPool, NeedLength, 'b');
		if (!pBuffer) {
			break;
		}

		//
		// try to query 
		//

		Status = ZwQuerySystemInformation(SystemModuleInformation, pBuffer, NeedLength, &NeedLength);
		if (NT_SUCCESS(Status)) {
			break;
		}

		NeedLength += 1000;

	} while (TRUE);

	if (!pBuffer) {
		return;
	}

	pSysModuleInfo = (PRTL_PROCESS_MODULES)pBuffer;

	do 
	{
		
		//
		// 这里procmon要保证他的驱动信息在第一个,
		// 所以这里进行的两轮处理,第一轮主要是找procmon的驱动.
		// 第二轮找除了procmon的其他驱动.
		//
		
		for (ULONG i = 0; i < pSysModuleInfo->NumberOfModules; i++)
		{
			BOOLEAN bIsOurModule = IsAddressInModule(ProcmonEnumLoadedModule, 
				pSysModuleInfo->Modules[i].ImageBase, pSysModuleInfo->Modules[i].ImageSize);
			if ((Round == 1 && !bIsOurModule) ||
				(Round == 0 && bIsOurModule)){
				ANSI_STRING ImageNameAnsi;
				UNICODE_STRING UnistrImageName;
				UNICODE_STRING UnistrFullName;

				
				//
				// 这里我们需要记录三个信息就够了
				// 分别为Properties ImageBase 和 ImageSize
				//
				
				ImageInfo.Properties |= 0x100;
				ImageInfo.ImageBase = pSysModuleInfo->Modules[i].ImageBase;
				ImageInfo.ImageSize = pSysModuleInfo->Modules[i].ImageSize;

				ImageNameAnsi.Length = (USHORT)strlen((CHAR*)pSysModuleInfo->Modules[i].FullPathName);
				ImageNameAnsi.Buffer = (PCHAR)pSysModuleInfo->Modules[i].FullPathName;

				Status = RtlAnsiStringToUnicodeString(&UnistrImageName, &ImageNameAnsi, TRUE);

				if (NT_SUCCESS(Status)){
					
					//
					// 如果路径已\\开头,说明是相对路径
					// 我们需要添加上Driver文件夹的路径.
					//
					
					if (UnistrImageName.Buffer[0] != L'\\' &&
						(UnistrFullName.MaximumLength = UnistrImageName.Length + UnistrDriversPath.Length,
						(UnistrFullName.Buffer = ProcmonAllocatePoolWithTag(NonPagedPool, UnistrFullName.MaximumLength, 'b')) != NULL)) {


						if (UnistrFullName.Buffer) {
							RtlCopyUnicodeString(&UnistrFullName, &UnistrDriversPath);
							RtlAppendUnicodeStringToString(&UnistrFullName, &UnistrImageName);
							LoadImageNotifyRoutine(&UnistrFullName, PsGetCurrentProcessId(), &ImageInfo);
							ExFreePoolWithTag(UnistrFullName.Buffer, 0);
						}
					}else{
						LoadImageNotifyRoutine(&UnistrImageName, PsGetCurrentProcessId(), &ImageInfo);
					}

					RtlFreeUnicodeString(&UnistrImageName);
				}
			}

		}
		++Round;
	} while (Round < 2);
}

VOID
ProcmonStart(
	_In_ PUNICODE_STRING RegistryPath
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	LARGE_INTEGER DueTime;
	LONG RuntimeSeconds = -1;
	LONG ThreadProfiling = 0;

	ExInitializeFastMutex(&gMutexVolume);
	
	//
	// 首先枚举下磁盘卷设备
	//
	
	ProcmonEnumAllVolumes();
	
	//
	// 从注册表中获取参数的值
	//
	
	if (ProcmonGetProcessParameter(RegistryPath, &ThreadProfiling, &RuntimeSeconds)){

		KeInitializeEvent(&gEventFileWriteFiled, 0, 0);
		RtlInitUnicodeString(&gUniStrSystemRoot, L"\\SystemRoot");
		gWriteFileFrqs = 450000000;

		//
		// 设置回调为写日志文件.
		//
		
		SetMessageProcessor(ProcmonCreatePbmFiles);

		//
		// RuntimeSeconds 为设置允许多长时间后退出.
		//
		
		DueTime.QuadPart = -RuntimeSeconds;
		if (RuntimeSeconds != -1){
			KeInitializeTimer(&gTimerRuntimes);
			KeInitializeDpc(&gDpcRuntimes, ProcmonRuntimeDpcRoutine, NULL);
			KeSetTimer(&gTimerRuntimes, DueTime, &gDpcRuntimes);
		}


		ProcmonControlProcMonitor(7);
		if (ThreadProfiling) {
			LARGE_INTEGER Profiling;

			Profiling.QuadPart = ThreadProfiling;
			ProcmonEnableThreadProfiling(Profiling);
		}
		ProcmonEnumLoadedModule();
	}
}


GUID DeviceClassGuid = {
	0x3A1380F4,
	0x708F,
	0x49DE,
	{0xB2, 0xEF, 0x04, 0xD2, 0x5E, 0xB0, 0x09, 0xD5}
};

HANDLE ghEventHandle;

NTSTATUS
DispatchProcmonExternalLoggerCreateClose(
	_In_ PDEVICE_OBJECT DeviceObject, 
	_In_ PIRP Irp
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{

	UNREFERENCED_PARAMETER(DeviceObject);

	Irp->IoStatus.Information = 0;
	Irp->IoStatus.Status = STATUS_SUCCESS;
	IofCompleteRequest(Irp, 0);
	return STATUS_SUCCESS;
}

NTSTATUS 
DispatchProcmonExternalLogger(
	_In_ PDEVICE_OBJECT pDeviceObject, 
	_In_ PIRP pIrp
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status = STATUS_INVALID_PARAMETER;
	PIO_STACK_LOCATION Irpsp;
	PPROCESSINFO_LIST pProcessInfo;
	PLOG_BUFFER pLogBuf;

	Irpsp = pIrp->Tail.Overlay.CurrentStackLocation;

	if (pDeviceObject == gDevProcmonExternalLogger){
		pIrp->IoStatus.Information = 0;
		Status = STATUS_SUCCESS;
#if 0
		if (Irpsp){
			if (Irpsp->Parameters.DeviceIoControl.IoControlCode == 0x95358200){
				pIrp->IoStatus.Information = 0;
				InputBufferLength = Irpsp->Parameters.DeviceIoControl.InputBufferLength;// InputBufferLength
				if (InputBufferLength < 0xFFF){

				}
			}
		}
#endif
	}

	if (pDeviceObject == gProcmonDebugLoggerDeviceObject){
		if (Irpsp){
			if (Irpsp->Parameters.DeviceIoControl.IoControlCode == 0x95358204 && 
				Irpsp->Parameters.DeviceIoControl.InputBufferLength < 0x1000){
				pProcessInfo = RefProcessInfo(PsGetCurrentProcessId(), TRUE);
				if (pProcessInfo){
					PVOID pLogInfo;
					pLogInfo = ProcmonGetLogEntryAndCopyFrameChain(
						MONITOR_TYPE_PROFILING,
						NOTIFY_PROFILING_DEBUG,
						pProcessInfo->Seq,
						259,
						Irpsp->Parameters.DeviceIoControl.InputBufferLength + 2,
						&pLogBuf);
					if (pLogInfo)
					{
						*(PUSHORT)pLogInfo = (USHORT)(Irpsp->Parameters.DeviceIoControl.InputBufferLength >> 1);
						RtlCopyMemory((PVOID)((ULONG_PTR)pLogInfo + sizeof(USHORT)), pIrp->AssociatedIrp.SystemBuffer, 
							Irpsp->Parameters.DeviceIoControl.InputBufferLength);
						ProcmonNotifyProcessLog(pLogBuf);
					}
					DerefProcessInfo(pProcessInfo);
					Status = STATUS_SUCCESS;
				}
			}
		}
	}
	pIrp->IoStatus.Information = 0;
	pIrp->IoStatus.Status = Status;
	IofCompleteRequest(pIrp, 0);
	return Status;
}

BOOLEAN
ProcmonCreateExternalLoggerDevice(
	_In_ PDRIVER_OBJECT DriverObject
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	PKEVENT pEvent;
	UNICODE_STRING UnistrDeviceName;
	UNICODE_STRING EventName;

	RtlInitUnicodeString(&UnistrDeviceName, PROCMON_EXTLOGGER_DEVICE_NAME);
	Status = WdmlibIoCreateDeviceSecure(
		DriverObject,
		0,
		&UnistrDeviceName,
		0x9535u,
		0x100u,
		0,
		&SDDL_DEVOBJ_KERNEL_ONLY,
		&DeviceClassGuid,
		&gDevProcmonExternalLogger);
	if (Status >= 0)
	{
		DriverObject->MajorFunction[IRP_MJ_CLOSE] = DispatchProcmonExternalLoggerCreateClose;
		DriverObject->MajorFunction[IRP_MJ_CREATE] = DispatchProcmonExternalLoggerCreateClose;
		DriverObject->MajorFunction[IRP_MJ_INTERNAL_DEVICE_CONTROL] = DispatchProcmonExternalLogger;
		DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = DispatchProcmonExternalLogger;
		RtlInitUnicodeString(&EventName, PROCMON_EXTLOGGER_ENABLE_EVENT_NAME);
		pEvent = IoCreateNotificationEvent(&EventName, &ghEventHandle);
		gProcmonExternalLoggerEnabledEvent = pEvent;
		if (pEvent){
			KeClearEvent(pEvent);
			return Status >= 0;
		}
		Status = STATUS_UNSUCCESSFUL;
	}
	return NT_SUCCESS(Status);
}

BOOLEAN
ProcmonCreateDebugLoggerDevice(
	_In_ PDRIVER_OBJECT DriverObject
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	NTSTATUS Status;
	UNICODE_STRING UnistrDeviceName;
	UNICODE_STRING DefaultSDDLString;
	UNICODE_STRING SymbolicLinkName;

	RtlInitUnicodeString(&DefaultSDDLString, L"D:P(A;; GA;;; AU)");
	RtlInitUnicodeString(&UnistrDeviceName, PROCMON_DEBUGLOGGER_DEVICE_NAME);
	Status = WdmlibIoCreateDeviceSecure(
		DriverObject,
		0,
		&UnistrDeviceName,
		0x9535u,
		0x100u,
		0,
		&DefaultSDDLString,
		&DeviceClassGuid,
		&gProcmonDebugLoggerDeviceObject);
	if (NT_SUCCESS(Status)){
		RtlInitUnicodeString(&SymbolicLinkName, PROCMON_DEBUGLOGGER_SYMBOL_NAME);
		Status = IoCreateSymbolicLink(&SymbolicLinkName, &UnistrDeviceName);
		if (!NT_SUCCESS(Status)){
			IoDeleteDevice(gProcmonDebugLoggerDeviceObject);
			gProcmonDebugLoggerDeviceObject = NULL;
		}
	}
	return NT_SUCCESS(Status);
}

BOOLEAN 
ProcmonInitDevice(
	_In_ PDRIVER_OBJECT DriverObject
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	BOOLEAN bRet;

	gDevProcmonExternalLogger = NULL;
	gProcmonDebugLoggerDeviceObject = NULL;
	bRet = ProcmonCreateExternalLoggerDevice(DriverObject);
	if (bRet)
		ProcmonCreateDebugLoggerDevice(DriverObject);
	return bRet;
}

NTSTATUS
DriverEntry (
    _In_ PDRIVER_OBJECT DriverObject,
    _In_ PUNICODE_STRING RegistryPath
    )
/*++

Routine Description:

    This is the initialization routine for this miniFilter driver.  This
    registers with FltMgr and initializes all global data structures.

Arguments:

    DriverObject - Pointer to driver object created by the system to
        represent this driver.

    RegistryPath - Unicode string identifying where the parameters for this
        driver are located in the registry.

Return Value:

    Routine can return non success error codes.

--*/
{
    NTSTATUS status;

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!DriverEntry: Entered\n") );

	
	//
	//  Get system version and check
	//
	
	PsGetVersion(NULL, NULL, &gBuildNumber, 0);
	if (gBuildNumber < 2600)
		return STATUS_NOT_SUPPORTED;

	gDriverObject = DriverObject;

	//
	// Start file mini-filter
	//
	
	status = ProcmonStartFileFilter(DriverObject);
    FLT_ASSERT( NT_SUCCESS( status ) );

    if (NT_SUCCESS( status )) {
		ProcmonInitialize();
		ProcmonProcessMonitorInit();
		ProcmonRegMonitorInit();
		ProcmonInitDevice(DriverObject);
		ProcmonStart(RegistryPath);
    }

    return status;
}

NTSTATUS
ProcmonUnload (
    _In_ FLT_FILTER_UNLOAD_FLAGS Flags
    )
/*++

Routine Description:

    This is the unload routine for this miniFilter driver. This is called
    when the minifilter is about to be unloaded. We can fail this unload
    request if this is not a mandatory unload indicated by the Flags
    parameter.

Arguments:

    Flags - Indicating if this is a mandatory unload.

Return Value:

    Returns STATUS_SUCCESS.

--*/
{
    UNREFERENCED_PARAMETER( Flags );

    PAGED_CODE();

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!procmonUnload: Entered\n") );

    FltUnregisterFilter( gFilterHandle );

    return STATUS_SUCCESS;
}




PVOID
ProcmonCollectFileOptInfo(
	_In_ PCFLT_RELATED_OBJECTS FltObjects,
	_In_ const PFLT_IO_PARAMETER_BLOCK Iopb,
	_Out_ PULONG pSize
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{

	PVOID pFileOptInfo = NULL;
	ULONG TotalLength = 0;

	switch (Iopb->MajorFunction)
	{
	case IRP_MJ_CREATE:
	{
		PTOKEN_USER pTokenUser = NULL;
		ULONG TokenSize = 0;
		HANDLE hToken = ProcmonGetProcessTokenHandle(TRUE);
		if (hToken){
			pTokenUser = ProcmonQueryTokenInformation(hToken, NULL, NULL, NULL);
			if (pTokenUser){
				TokenSize = RtlLengthSid(pTokenUser->User.Sid);
			}
			ZwClose(hToken);
		}
		TotalLength = TokenSize + sizeof(LOG_FILE_CREATE);
		*pSize = TotalLength;
		pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
		if (pFileOptInfo){
			PLOG_FILE_CREATE pCreateFileOpt = (PLOG_FILE_CREATE)pFileOptInfo;
			pCreateFileOpt->DesiredAccess = Iopb->Parameters.Create.SecurityContext->DesiredAccess;
			pCreateFileOpt->UserTokenLength = TokenSize;
			if (TokenSize){
				RtlCopyMemory(pCreateFileOpt + 1, pTokenUser->User.Sid, TokenSize);
			}
		}

		if (pTokenUser){
			ExFreePoolWithTag(pTokenUser, 0);
		}
	}
		break;

	case IRP_MJ_SET_INFORMATION:
	{
		if (Iopb->Parameters.SetFileInformation.FileInformationClass == FileRenameInformation ||
			Iopb->Parameters.SetFileInformation.FileInformationClass == FileRenameInformationEx ||
			Iopb->Parameters.SetFileInformation.FileInformationClass == FileRenameInformationExBypassAccessCheck){

			NTSTATUS Status;
			PFILE_RENAME_INFORMATION pRenameInfo = (PFILE_RENAME_INFORMATION)Iopb->Parameters.SetFileInformation.InfoBuffer; 
			PFLT_FILE_NAME_INFORMATION pFileNameInformation = NULL;
			USHORT FileNameLength = 0;

			TotalLength = Iopb->Parameters.SetFileInformation.Length + sizeof(USHORT);

			Status = FltGetDestinationFileNameInformation(
				FltObjects->Instance,
				FltObjects->FileObject,
				pRenameInfo->RootDirectory,
				pRenameInfo->FileName,
				pRenameInfo->FileNameLength,
				FLT_FILE_NAME_QUERY_DEFAULT | FLT_FILE_NAME_NORMALIZED,
				&pFileNameInformation);
			if (NT_SUCCESS(Status)){
				FileNameLength = pFileNameInformation->Name.Length;
				TotalLength += FileNameLength;
			}else{
				pFileNameInformation = NULL;
				FileNameLength = 0;
			}

			*pSize = TotalLength;
			pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
			if (pFileOptInfo){
				RtlCopyMemory(pFileOptInfo, Iopb->Parameters.SetFileInformation.InfoBuffer, 
					Iopb->Parameters.SetFileInformation.Length);
				*(PUSHORT)((ULONG_PTR)pFileOptInfo + Iopb->Parameters.SetFileInformation.Length) = FileNameLength >> 1;
				if (FileNameLength){
					RtlCopyMemory((PVOID)((ULONG_PTR)pFileOptInfo + Iopb->Parameters.SetFileInformation.Length + sizeof(USHORT)),
						pFileNameInformation->Name.Buffer,
						FileNameLength
					);
				}
			}

			if (pFileNameInformation){
				FltReleaseFileNameInformation(pFileNameInformation);
			}
		}else{
			TotalLength = Iopb->Parameters.SetFileInformation.Length;

			*pSize = TotalLength;
			pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
			if (pFileOptInfo){
				RtlCopyMemory(pFileOptInfo, Iopb->Parameters.SetFileInformation.InfoBuffer,
					Iopb->Parameters.SetFileInformation.Length);
			}
		}
	}
		break;
	case IRP_MJ_SET_VOLUME_INFORMATION:
	{
		TotalLength = Iopb->Parameters.SetVolumeInformation.Length;
		*pSize = TotalLength;
		pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
		if (pFileOptInfo) {
			RtlCopyMemory(pFileOptInfo, Iopb->Parameters.SetVolumeInformation.VolumeBuffer,
				Iopb->Parameters.SetVolumeInformation.Length);
		}
	}
		break;
	case IRP_MJ_DIRECTORY_CONTROL:
	{
		if (Iopb->MinorFunction == IRP_MN_QUERY_DIRECTORY) {
			USHORT FileNameLength = 0;

			if (Iopb->Parameters.DirectoryControl.QueryDirectory.FileName){
				FileNameLength = Iopb->Parameters.DirectoryControl.QueryDirectory.FileName->Length;
				TotalLength = FileNameLength;
			}

			TotalLength += sizeof(LOG_FILE_NAME_COMMON);
			*pSize = TotalLength;
			pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
			if (pFileOptInfo) {
				PLOG_FILE_NAME_COMMON pFileNameOpt = (PLOG_FILE_NAME_COMMON)pFileOptInfo;
				pFileNameOpt->FileNameLength = FileNameLength >> 1;
				if (FileNameLength) {
					RtlCopyMemory(pFileNameOpt + 1, Iopb->Parameters.DirectoryControl.QueryDirectory.FileName->Buffer,
						FileNameLength);
				}
			}
		}
	}
		break;
	case IRP_MJ_FILE_SYSTEM_CONTROL:
	{
		ULONG FsControl = Iopb->Parameters.FileSystemControl.Common.FsControlCode;
		if (FsControl == FSCTL_OFFLOAD_READ || FsControl == FSCTL_OFFLOAD_WRITE){
			TotalLength = Iopb->Parameters.FileSystemControl.Buffered.InputBufferLength;
			*pSize = TotalLength;
			pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
			if (pFileOptInfo){
				RtlCopyMemory(pFileOptInfo, Iopb->Parameters.FileSystemControl.Buffered.SystemBuffer, TotalLength);
			}
		}
	}
		break;
	case IRP_MJ_LOCK_CONTROL:
	{
		LONGLONG LockCtlLength = 0;
		if (Iopb->Parameters.LockControl.Length){
			LockCtlLength = Iopb->Parameters.LockControl.Length->QuadPart;
		}

		TotalLength = sizeof(LOG_FILE_LOCKCONTROL);
		*pSize = TotalLength;
		pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
		if (pFileOptInfo) {
			RtlCopyMemory(pFileOptInfo, &LockCtlLength, TotalLength);
		}
	}
		break;
	case IRP_MJ_ACQUIRE_FOR_MOD_WRITE:
	{
		TotalLength = sizeof(LOG_FILE_ACQUIREFORMODIFIEDPAGEWRITER);
		*pSize = TotalLength;
		pFileOptInfo = ProcmonAllocatePoolWithTag(NonPagedPool, TotalLength, 'H');
		if (pFileOptInfo) {
			RtlCopyMemory(pFileOptInfo, Iopb->Parameters.AcquireForModifiedPageWriter.EndingOffset, TotalLength);
		}
	}
		break;
	default:
		break;
	}

	if (!pFileOptInfo){
		*pSize = 0;
	}else{
		*pSize = min(TotalLength, 0xFFFF);//TotalLength > 0xFFFF ? 0xFFFF : TotalLength;
	}

	return pFileOptInfo;

}

/*************************************************************************
    MiniFilter callback routines.
*************************************************************************/
FLT_PREOP_CALLBACK_STATUS
ProcmonPreOperation (
    _Inout_ PFLT_CALLBACK_DATA Data,
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _Flt_CompletionContext_Outptr_ PVOID *CompletionContext
    )
/*++

Routine Description:

    This routine is a pre-operation dispatch routine for this miniFilter.

    This is non-pageable because it could be called on the paging path

Arguments:

    Data - Pointer to the filter callbackData that is passed to us.

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance, its associated volume and
        file object.

    CompletionContext - The context for the completion routine for this
        operation.

Return Value:

    The return value is the status of the operation.

--*/
{
	NTSTATUS Status;
	PUNICODE_STRING pStrFileName = NULL;
	PFLT_FILE_NAME_INFORMATION FileNameInformation = NULL;
	BOOLEAN bSetTopIrql = FALSE;
	PIRP pTopIrpSaved = NULL;
	ULONG FullNameLength;
	HANDLE ProcessId;
	FLT_PREOP_CALLBACK_STATUS FltStatus = FLT_PREOP_SUCCESS_NO_CALLBACK;

	//
	// If major code is not shutdown
	// record the file operation and send to ring3
	//
	
	if (Data->Iopb->MajorFunction != IRP_MJ_SHUTDOWN){
		
		//
		// Invalid fileobject or User turn off the file monitor
		// do not call the post callback 
		//
		
		if (!FltObjects->FileObject || !FlagOn(gFlags, 2)){
			return FLT_PREOP_SUCCESS_NO_CALLBACK;
		}

		//
		// #define PASSIVE_LEVEL 0                 // Passive release level
		// #define LOW_LEVEL 0                     // Lowest interrupt level
		// #define APC_LEVEL 1                     // APC interrupt level
		// #define DISPATCH_LEVEL 2                // Dispatcher level
		//

		if (FsRtlIsPagingFile(FltObjects->FileObject) && KeGetCurrentIrql() == APC_LEVEL){
			pStrFileName = FindPagingFileNameInList(FltObjects->FileObject);
		}else{
			Status = FltGetFileNameInformation(Data,
				FLT_FILE_NAME_NORMALIZED | FLT_FILE_NAME_QUERY_ALWAYS_ALLOW_CACHE_LOOKUP,
				&FileNameInformation);
			if (NT_SUCCESS(Status)){
				
				//
				// duplicate the string
				//
				
				pStrFileName = ProcmonDuplicateUnicodeString(NonPagedPool, &FileNameInformation->Name, '1');
				FltReleaseFileNameInformation(FileNameInformation);
			}else{
				
				//
				// try use FLT_FILE_NAME_OPENED
				//
				
				Status = FltGetFileNameInformation(Data,
					FLT_FILE_NAME_OPENED | FLT_FILE_NAME_QUERY_ALWAYS_ALLOW_CACHE_LOOKUP,
					&FileNameInformation);
				if (NT_SUCCESS(Status)) {

					//
					// duplicate the string
					//

					pStrFileName = ProcmonDuplicateUnicodeString(NonPagedPool, &FileNameInformation->Name, '1');
					FltReleaseFileNameInformation(FileNameInformation);

				}else{
					
					//
					// Is paging file?
					//
					
					if (FsRtlIsPagingFile(FltObjects->FileObject) && 
						IoGetTopLevelIrp() == (PIRP)FltObjects->FileObject->FileName.Buffer ){
						
						//
						// Here we already add the paging file name to list
						//
						
						pStrFileName = FindPagingFileNameInList(FltObjects->FileObject);
					}else{

						if (FsRtlIsPagingFile(FltObjects->FileObject)){
							
							//
							// Save the old top Irp of this thread
							//
							
							pTopIrpSaved = IoGetTopLevelIrp();
							
							//
							// Set the our file name buffer
							//
							
							IoSetTopLevelIrp((PIRP)FltObjects->FileObject->FileName.Buffer);
							bSetTopIrql = TRUE;
						}
						
						//
						// Get volume name length
						//
						
						FltGetVolumeName(FltObjects->Volume, NULL, &FullNameLength);
						
						//
						// Calculate full file name length
						//
						
						FullNameLength += FltObjects->FileObject->FileName.Length;
						
						//
						// Allocate new buffer for full name
						//
						
						pStrFileName = (PUNICODE_STRING)ProcmonAllocatePoolWithTag(NonPagedPool, 
							FullNameLength + sizeof(UNICODE_STRING), '1');
						if (pStrFileName){
							
							//
							// Initialize the new string
							//
							
							pStrFileName->MaximumLength = (USHORT)FullNameLength;
							pStrFileName->Buffer = (PWCH)(pStrFileName + 1);
							
							//
							// Get the volume name
							//
							
							FltGetVolumeName(FltObjects->Volume, pStrFileName, &FullNameLength);
							RtlAppendUnicodeStringToString(pStrFileName, &FltObjects->FileObject->FileName);
						}

						
						//
						// Restore the top Irp
						//
						
						if (bSetTopIrql){
							IoSetTopLevelIrp(pTopIrpSaved);
						}
					}
				}
			}
		}
		
		//
		// Get file name finish
		//
		
		if (pStrFileName){
			
			BOOLEAN bNotAsyn;
			PPROCESSINFO_LIST pProcessInfo;
			ULONG FileInfoSize = 0, TotalLength;
			PVOID pFileOptInfo;
			UCHAR MinjorFunction;
			USHORT NotifyType;
			PLOG_FILE_OPT pLogEntry;
			LONG Seq;
			PLOG_BUFFER pLogBuf;

			//
			// Here we get the file name 
			//

			if (FsRtlIsPagingFile(FltObjects->FileObject)){
				
				//
				// Add to paging file object list
				//
				
				AddToPagingFileNameList(FltObjects->FileObject, pStrFileName);

			}

			bNotAsyn = !FlagOn(Data->Iopb->IrpFlags, IRP_SYNCHRONOUS_PAGING_IO) ? TRUE : FALSE;
			ProcessId = PsGetCurrentProcessId();
			pProcessInfo = RefProcessInfo(ProcessId, bNotAsyn);
			if (pProcessInfo && pProcessInfo->bInit){
				if (FlagOn(Data->Flags, FLTFL_CALLBACK_DATA_FAST_IO_OPERATION)){
					Data->IoStatus.Status = STATUS_SUCCESS;
				}

				pFileOptInfo = ProcmonCollectFileOptInfo(FltObjects, Data->Iopb, &FileInfoSize);
				if (Data->Iopb->MajorFunction == IRP_MJ_QUERY_INFORMATION ||
					Data->Iopb->MajorFunction == IRP_MJ_SET_INFORMATION ||
					Data->Iopb->MajorFunction == IRP_MJ_QUERY_VOLUME_INFORMATION) {
					MinjorFunction = (UCHAR)Data->Iopb->Parameters.QueryFileInformation.FileInformationClass;
				}else{

					//
					// IRP_MJ_QUERY_INFORMATION Or IRP_MJ_SET_INFORMATION
					//

					MinjorFunction = Data->Iopb->MinorFunction;
				}

				TotalLength = pStrFileName->Length + FileInfoSize + sizeof(LOG_FILE_OPT);
				
				//
				// like IRP_MJ_NETWORK_QUERY_OPEN will be failed
				//
				
				NotifyType = (UCHAR)(Data->Iopb->MajorFunction + 20);
				pLogEntry = (PLOG_FILE_OPT)ProcmonGetLogEntryAndSeq(
					!FsRtlIsPagingFile(FltObjects->FileObject),
					MONITOR_TYPE_FILE,
					NotifyType,
					pProcessInfo->Seq,
					STATUS_PENDING,
					TotalLength,
					&Seq,
					&pLogBuf);
				if (pLogEntry) {
					pLogEntry->MinorFunction = MinjorFunction;
					pLogEntry->IopbFlag = Data->Iopb->IrpFlags | (Data->Iopb->OperationFlags << 20);
					pLogEntry->Flags = Data->Flags;
#if 0
					pLogEntry->Argument1 = Data->Iopb->Parameters.Others.Argument1;
					pLogEntry->Argument2 = Data->Iopb->Parameters.Others.Argument2;
					pLogEntry->Argument3 = Data->Iopb->Parameters.Others.Argument3;
					pLogEntry->Argument4 = Data->Iopb->Parameters.Others.Argument4;
					pLogEntry->Argument5 = Data->Iopb->Parameters.Others.Argument5;
					pLogEntry->Argument6 = Data->Iopb->Parameters.Others.Argument6;
#endif
					pLogEntry->FltParameter.Others.Argument1 = Data->Iopb->Parameters.Others.Argument1;
					pLogEntry->FltParameter.Others.Argument2 = Data->Iopb->Parameters.Others.Argument2;
					pLogEntry->FltParameter.Others.Argument3 = Data->Iopb->Parameters.Others.Argument3;
					pLogEntry->FltParameter.Others.Argument4 = Data->Iopb->Parameters.Others.Argument4;
					pLogEntry->FltParameter.Others.Argument5 = Data->Iopb->Parameters.Others.Argument5;
					pLogEntry->FltParameter.Others.Argument6 = Data->Iopb->Parameters.Others.Argument6;
					pLogEntry->NameLength = pStrFileName->Length >> 1;
					RtlCopyMemory(pLogEntry->Name, pStrFileName->Buffer, pStrFileName->Length);
					if (FileInfoSize) {
						RtlCopyMemory((PVOID)((ULONG_PTR)pLogEntry->Name + pStrFileName->Length),
							pFileOptInfo, FileInfoSize);
						ExFreePoolWithTag(pFileOptInfo, 0);
					}

					ProcmonNotifyProcessLog(pLogBuf);
					*CompletionContext = (PVOID)Seq;
					FltStatus = FLT_PREOP_SUCCESS_WITH_CALLBACK;
				}
				DerefProcessInfo(pProcessInfo);
			}

			ExFreePoolWithTag(pStrFileName, 0);
			return FltStatus;

		}else{

			//
			// Hmm. some error occurred in getting filename
			// log it
			//

			KdPrint(("procmon!%s failed to get filename", __FUNCTION__));
			//__debugbreak();
			return FLT_PREOP_SUCCESS_NO_CALLBACK;
		}
	}
	
	//
	// if monitor process exit. 
	// we need turn off the monitor driver
	//
	
	if (!gCurrentProcess){

		ProcmonProcessExitOff();
		return FLT_PREOP_SUCCESS_NO_CALLBACK;
		
	}

	return FLT_PREOP_SUCCESS_NO_CALLBACK;
	
}



VOID
ProcmonOperationStatusCallback (
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_ PFLT_IO_PARAMETER_BLOCK ParameterSnapshot,
    _In_ NTSTATUS OperationStatus,
    _In_ PVOID RequesterContext
    )
/*++

Routine Description:

    This routine is called when the given operation returns from the call
    to IoCallDriver.  This is useful for operations where STATUS_PENDING
    means the operation was successfully queued.  This is useful for OpLocks
    and directory change notification operations.

    This callback is called in the context of the originating thread and will
    never be called at DPC level.  The file object has been correctly
    referenced so that you can access it.  It will be automatically
    dereferenced upon return.

    This is non-pageable because it could be called on the paging path

Arguments:

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance, its associated volume and
        file object.

    RequesterContext - The context for the completion routine for this
        operation.

    OperationStatus -

Return Value:

    The return value is the status of the operation.

--*/
{
    UNREFERENCED_PARAMETER( FltObjects );

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!procmonOperationStatusCallback: Entered\n") );

    PT_DBG_PRINT( PTDBG_TRACE_OPERATION_STATUS,
                  ("procmon!procmonOperationStatusCallback: Status=%08x ctx=%p IrpMj=%02x.%02x \"%s\"\n",
                   OperationStatus,
                   RequesterContext,
                   ParameterSnapshot->MajorFunction,
                   ParameterSnapshot->MinorFunction,
                   FltGetIrpName(ParameterSnapshot->MajorFunction)) );
}

FLT_POSTOP_CALLBACK_STATUS
ProcmonPostOperation (
    _Inout_ PFLT_CALLBACK_DATA Data,
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _In_opt_ PVOID CompletionContext,
    _In_ FLT_POST_OPERATION_FLAGS Flags
    )
/*++

Routine Description:

    This routine is the post-operation completion routine for this
    miniFilter.

    This is non-pageable because it may be called at DPC level.

Arguments:

    Data - Pointer to the filter callbackData that is passed to us.

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance, its associated volume and
        file object.

    CompletionContext - The completion context set in the pre-operation routine.

    Flags - Denotes whether the completion is successful or is being drained.

Return Value:

    The return value is the status of the operation.

--*/
{
	KIRQL Irql = KeGetCurrentIrql();

	if (!FlagOn(Flags, FLTFL_POST_OPERATION_DRAINING)){
		if (Irql == DISPATCH_LEVEL){
			PFILEOPT_WORKQUEUEITEM pFileOptWorkItem = ProcmonAllocatePoolWithTag(NonPagedPool, sizeof(FILEOPT_WORKQUEUEITEM), 'I');
			if (pFileOptWorkItem){
				pFileOptWorkItem->WorkItem.Parameter = pFileOptWorkItem;
				pFileOptWorkItem->WorkItem.List.Flink = NULL;
				pFileOptWorkItem->WorkItem.WorkerRoutine = ProcmonFilePostOptWorkerRoutine;
				pFileOptWorkItem->Thread = Data->Thread;
				pFileOptWorkItem->MajorFunction = Data->Iopb->MajorFunction;
				pFileOptWorkItem->Time = ProcmonGetTime();
				pFileOptWorkItem->IoStatus = Data->IoStatus;
				pFileOptWorkItem->CompletionContext = CompletionContext;
				pFileOptWorkItem->Flags = Data->Flags;
				ExQueueWorkItem(&pFileOptWorkItem->WorkItem, DelayedWorkQueue);
			}
		}else{			
			LARGE_INTEGER Time = {0};
			ProcmonFilePostOptRoutine(Data->Thread, Data->Iopb->MajorFunction,
				&Data->IoStatus, Data->Iopb, CompletionContext, Time, Data->Flags);
		}
	}
	if (Data->Iopb->MajorFunction == IRP_MJ_FILE_SYSTEM_CONTROL && 
		Data->Iopb->Parameters.FileSystemControl.Common.FsControlCode == FSCTL_DISMOUNT_VOLUME){
		FltDetachVolume(gFilterHandle, FltObjects->Volume, NULL);
	}
		
	return FLT_POSTOP_FINISHED_PROCESSING;
}


FLT_PREOP_CALLBACK_STATUS
ProcmonPreOperationNoPostOperation (
    _Inout_ PFLT_CALLBACK_DATA Data,
    _In_ PCFLT_RELATED_OBJECTS FltObjects,
    _Flt_CompletionContext_Outptr_ PVOID *CompletionContext
    )
/*++

Routine Description:

    This routine is a pre-operation dispatch routine for this miniFilter.

    This is non-pageable because it could be called on the paging path

Arguments:

    Data - Pointer to the filter callbackData that is passed to us.

    FltObjects - Pointer to the FLT_RELATED_OBJECTS data structure containing
        opaque handles to this filter, instance, its associated volume and
        file object.

    CompletionContext - The context for the completion routine for this
        operation.

Return Value:

    The return value is the status of the operation.

--*/
{
    UNREFERENCED_PARAMETER( Data );
    UNREFERENCED_PARAMETER( FltObjects );
    UNREFERENCED_PARAMETER( CompletionContext );

    PT_DBG_PRINT( PTDBG_TRACE_ROUTINES,
                  ("procmon!procmonPreOperationNoPostOperation: Entered\n") );

    // This template code does not do anything with the callbackData, but
    // rather returns FLT_PREOP_SUCCESS_NO_CALLBACK.
    // This passes the request down to the next miniFilter in the chain.

    return FLT_PREOP_SUCCESS_NO_CALLBACK;
}


BOOLEAN
procmonDoRequestOperationStatus(
    _In_ PFLT_CALLBACK_DATA Data
    )
/*++

Routine Description:

    This identifies those operations we want the operation status for.  These
    are typically operations that return STATUS_PENDING as a normal completion
    status.

Arguments:

Return Value:

    TRUE - If we want the operation status
    FALSE - If we don't

--*/
{
    PFLT_IO_PARAMETER_BLOCK iopb = Data->Iopb;

    //
    //  return boolean state based on which operations we are interested in
    //

    return (BOOLEAN)

            //
            //  Check for oplock operations
            //

             (((iopb->MajorFunction == IRP_MJ_FILE_SYSTEM_CONTROL) &&
               ((iopb->Parameters.FileSystemControl.Common.FsControlCode == FSCTL_REQUEST_FILTER_OPLOCK)  ||
                (iopb->Parameters.FileSystemControl.Common.FsControlCode == FSCTL_REQUEST_BATCH_OPLOCK)   ||
                (iopb->Parameters.FileSystemControl.Common.FsControlCode == FSCTL_REQUEST_OPLOCK_LEVEL_1) ||
                (iopb->Parameters.FileSystemControl.Common.FsControlCode == FSCTL_REQUEST_OPLOCK_LEVEL_2)))

              ||

              //
              //    Check for directy change notification
              //

              ((iopb->MajorFunction == IRP_MJ_DIRECTORY_CONTROL) &&
               (iopb->MinorFunction == IRP_MN_NOTIFY_CHANGE_DIRECTORY))
             );
}

VOID 
EnableLogger(
	_In_ BOOLEAN bEnable
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	if (bEnable){
		KeQuerySystemTime(&gMonitorStartTime);
		gMonitorStartCounter = KeQueryPerformanceCounter(&gPerformanceFrequency);
		KeCancelTimer(&gTimerProcessLogData);
		gbReady = FALSE;
		gbFinish = FALSE;

	}else{

		PLIST_ENTRY pEntry;
		ExAcquireFastMutex(&gMutexLogList);
		for (pEntry = gLogListHead.Flink; 
			gLogListHead.Flink != &gLogListHead; 
			pEntry = gLogListHead.Flink)
		{
			PLOG_BUFFER pLogBuffer = CONTAINING_RECORD(pEntry, LOG_BUFFER, List);
			//RemoveEntryList(&gLogListHead);
			RemoveHeadList(&gLogListHead);
			ExFreeToNPagedLookasideList(&gNPagedLooksideListLogBuffer, pLogBuffer);

		}
		ExReleaseFastMutex(&gMutexLogList);
	}
}

VOID 
EnableExtLogEvent(
	UCHAR Flags)
{
	if (gProcmonExternalLoggerEnabledEvent){
		if (Flags)
			KeSetEvent(gProcmonExternalLoggerEnabledEvent, 0, FALSE);
		else
			KeClearEvent(gProcmonExternalLoggerEnabledEvent);
	}
}

#define PROCMON_ENBALE_PROCESS_MON			1
#define PROCMON_ENBALE_FILE_MON				2
#define PROCMON_ENBALE_REG_MON				4
#define PROCMON_ENBALE_REG_MON1				8
#define PROCMON_ENBALE_EXTLOG_EVENT			0x10

VOID 
ProcmonControlProcMonitor(
	_In_ ULONG Flags
)
/*++

Routine Description:

    .

Arguments:

	 - 

Return Value:

    Routine can return non success error codes.

--*/
{
	if (Flags)
		EnableLogger(TRUE);
	EnableProcessMonitor(Flags & 1);
	EnableRegMonitor(Flags & 0xC);
	if (Flags & 2)
		EnableFileMonitor(TRUE);
	EnableExtLogEvent(Flags & 0x10);
	if (!Flags)
		EnableLogger(FALSE);
	gFlags = Flags;
}

VOID 
ProcmonProcessExitOff(
	VOID
)
/*++

Routine Description:

    .

Arguments:

	 None

Return Value:

	None

--*/
{
	if (gFlags){
		ProcmonCollectProcessAndSystemPerformanceData();
		SetMessageProcessor(ProcmonWriteToPbmFile);
		KeWaitForSingleObject(&gEventFileWriteFiled, 0, 0, 0, NULL);
		ProcmonControlProcMonitor(0);
	}
}