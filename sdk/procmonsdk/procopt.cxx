
#include "pch.hpp"
#include "procmgr.hpp"
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
		PROCMGR().Remove(pEvent);
		break;
	case NOTIFY_IMAGE_LOAD:
	{
		PLOG_LOADIMAGE_INFO pImageLoadInfo = TO_EVENT_DATA(PLOG_LOADIMAGE_INFO, pEntry);
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

CString CProcEvent::GetPath()
{
	PLOG_ENTRY pEntry = reinterpret_cast<PLOG_ENTRY>(getPreLog().GetBuffer());

	switch (pEntry->NotifyType)
	{
	case NOTIFY_PROCESS_INIT:
	case NOTIFY_PROCESS_EXIT:
		break;
	case NOTIFY_PROCESS_CREATE:
	{
		CProcInfoView clsView(this);
		return clsView.GetImagePath();
	}
	break;
	case NOTIFY_IMAGE_LOAD:
	{
		PLOG_LOADIMAGE_INFO pImageLoadInfo = TO_EVENT_DATA(PLOG_LOADIMAGE_INFO, pEntry);
		CModule Mod(pImageLoadInfo);
		return Mod.GetPath();
	}
	break;
	default:
		break;
	}

	return TEXT("");
}

CString CProcEvent::GetDetail()
{
	PLOG_ENTRY pEntry = reinterpret_cast<PLOG_ENTRY>(getPreLog().GetBuffer());
	CString strDetail;

	switch (pEntry->NotifyType)
	{
	case NOTIFY_PROCESS_CREATE:
	{
		CProcInfoView clsView(this);

		strDetail.Format(TEXT("PID: %d\r\nCommand Line:%s"), 
				clsView.GetProcessId(), 
				clsView.GetCommandLine().GetBuffer());
		
	}
	break;
	default:
		strDetail = TEXT("TODO");
		break;
	}
	
	return strDetail;
}
