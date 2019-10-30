
#include "pch.hpp"
#include "procmgr.hpp"
#include "eventmgr.hpp"
#include "procopt.hpp"

BOOL CProcOpt::Process(const CRefPtr<CLogEvent> pEvent)
{
	PLOG_ENTRY pEntry = (PLOG_ENTRY)pEvent->getPreLog().GetBuffer();
	if (pEntry->MonitorType != MONITOR_TYPE_PROCESS) {
		return TRUE;
	}

	switch (pEntry->NotifyType)
	{
	case NOTIFY_PROCESS_INIT:
	case NOTIFY_PROCESS_CREATE:
	{
		CRefPtr<CProcess> pProcess = new CProcess(pEvent);
		PROCMGR().Insert(pProcess);
	}
		break;
	case NOTIFY_PROCESS_EXIT:
		PROCMGR().Remove(pEntry->ProcessSeq);
		break;
	case NOTIFY_IMAGE_LOAD:
	{
		PLOG_LOADIMAGE_INFO pImageLoadInfo = (PLOG_LOADIMAGE_INFO)((ULONG_PTR)(pEntry + 1) +
			pEntry->nFrameChainCounts * sizeof(PVOID));
		PROCMGR().InsertModule(pEntry->ProcessSeq, pImageLoadInfo);
	}
		break;
	default:
		break;
	}

	return TRUE;
}

BOOL CProcOpt::IsType(ULONG MonitorType)
{
	return MonitorType == MONITOR_TYPE_PROCESS;
}

