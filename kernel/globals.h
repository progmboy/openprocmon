#pragma once

#include <ntifs.h>
#include <fltKernel.h>

extern ULONG gBuildNumber;
extern PFLT_FILTER gFilterHandle;
extern ULONG gFlags;
extern PDRIVER_OBJECT gDriverObject;
extern PFLT_PORT gClientProcessPathPort;
extern PVOID gSelfImageBase;
extern ULONG gSelfImageSize;

extern KEVENT gEventProcessData;
extern KTIMER gTimerProcessLogData;
extern KDPC gDpcProcessData;
extern PEPROCESS gCurrentProcess;
extern HANDLE gProcessId;
extern KSPIN_LOCK gFileNameInfoListSpinLock;

extern LIST_ENTRY gThreadInfoList;
extern FAST_MUTEX gThreadInfoMutex;
extern NPAGED_LOOKASIDE_LIST gNPagedLooksideListThreadInfo;

extern FAST_MUTEX gMutexVolume;
extern UNICODE_STRING gUniStrSystemRoot;

extern BOOLEAN gbReady;
extern BOOLEAN gbFinish;