
#include "globals.h"

ULONG gBuildNumber = 0;
PFLT_FILTER gFilterHandle = NULL;
ULONG gFlags;
PDRIVER_OBJECT gDriverObject = NULL;
PFLT_PORT gClientProcessPathPort;
PVOID gSelfImageBase = NULL;
ULONG gSelfImageSize;

KEVENT gEventProcessData;
KTIMER gTimerProcessLogData;
KDPC gDpcProcessData;
PEPROCESS gCurrentProcess = NULL;
HANDLE gProcessId;
KSPIN_LOCK gFileNameInfoListSpinLock;

LIST_ENTRY gThreadInfoList;
FAST_MUTEX gThreadInfoMutex;
NPAGED_LOOKASIDE_LIST gNPagedLooksideListThreadInfo;

FAST_MUTEX gMutexVolume;
UNICODE_STRING gUniStrSystemRoot;

BOOLEAN gbReady = FALSE;
BOOLEAN gbFinish = FALSE;