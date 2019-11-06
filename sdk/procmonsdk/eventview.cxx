
#include "pch.hpp"
#include "eventview.hpp"
#include "process.hpp"
#include "utils.hpp"
#include "strmaps.hpp"

CEventView::CEventView()
{

}

CEventView::~CEventView()
{

}

void CEventView::SetEventOpt(CRefPtr<CLogEvent> pEvent)
{
	m_EventView = pEvent;
}

void CEventView::SnapProcess(CRefPtr<CProcess> pProcess)
{
	m_ProcView = pProcess->GetEvent();
	m_ModuleInfo = pProcess->GetModuleList();
	m_ProcInfo = pProcess->GetProcInfo();
}

DWORD CEventView::GetSeqNumber()
{
	return m_EventView.GetSeqNumber();
}

DWORD CEventView::GetEventClass()
{
	return m_EventView.GetEventClass();
}

DWORD CEventView::GetEventOperator()
{
	return m_EventView.GetOperator();
}

LARGE_INTEGER CEventView::GetStartTime()
{

	return m_EventView.GetStartTime();
}

LARGE_INTEGER CEventView::GetCompleteTime()
{
	return m_EventView.GetCompleteTime();
}

CString CEventView::GetPath()
{
	return m_EventView.GetPath();
}

CString CEventView::GetDetail()
{
	return m_EventView.GetDetail();
}

NTSTATUS CEventView::GetResult()
{
	return m_EventView.GetResult();
}

DWORD CEventView::GetCallStack(std::vector<PVOID>& callStacks)
{
	return m_EventView.GetCallStack(callStacks);
}

DWORD CEventView::GetProcessSeq()
{
	return m_EventView.GetProcessSeq();
}

PLOG_ENTRY CEventView::GetPreEventEntry()
{
	if (!m_EventView.GetEvent().IsNull()){
		if (m_EventView.GetEvent()->getPreLog().GetBufferLen()){
			return reinterpret_cast<PLOG_ENTRY>(m_EventView.GetEvent()->getPreLog().GetBuffer());
		}
	}
	return NULL;
	
}

PLOG_ENTRY CEventView::GetPostEventEntry()
{
	if (!m_EventView.GetEvent().IsNull()) {
		if (m_EventView.GetEvent()->getPostLog().GetBufferLen()) {
			return reinterpret_cast<PLOG_ENTRY>(m_EventView.GetEvent()->getPostLog().GetBuffer());
		}
	}
	return NULL;
}

DWORD CEventView::GetProcessId()
{
	return m_ProcView.GetProcessId();
}

DWORD CEventView::GetSessionId()
{
	return m_ProcView.GetSessionId();
}

DWORD CEventView::GetThreadId()
{
	return m_EventView.GetThreadId();
}

DWORD CEventView::GetParentProcessId()
{
	return m_ProcView.GetParentProcessId();
}

LUID CEventView::GetAuthId()
{
	return m_ProcView.GetAuthId();
}

CString CEventView::GetUserName()
{
	return m_ProcView.GetUserName();
}

PSID CEventView::GetUserSid()
{
	return m_ProcView.GetUserSid();
}

DWORD CEventView::GetIntegrity()
{
	return m_ProcView.GetIntegrity();
}

BOOL CEventView::IsVirtualize()
{
	return m_ProcView.IsVirtualize();
}

CString CEventView::GetProcessName()
{
	return m_ProcView.GetProcessName();
}

CString CEventView::GetImagePath()
{
	return m_ProcView.GetImagePath();
}

CString CEventView::GetCommandLine()
{
	return m_ProcView.GetCommandLine();
}

BOOL CEventView::IsWow64()
{
	return m_ProcView.IsWow64();
}


CBuffer& CEventView::GetProcIcon(BOOL bSmall)
{
	if (bSmall) {
		return m_ProcInfo->GetSmallIcon();
	}else{
		return m_ProcInfo->GetLargeIcon();
	}
}

const CString& CEventView::GetCompanyName()
{
	return m_ProcInfo->GetCompanyName();
}

const CString& CEventView::GetDisplayName()
{
	return m_ProcInfo->GetDisplayName();
}

const CString& CEventView::GetVersion()
{
	return m_ProcInfo->GetVersion();
}

std::vector<CModule>& CEventView::GetModuleList()
{
	return m_ModuleInfo;
}

BOOL CEventView::IsProcessExit()
{
	ULONG Seq = m_EventView.GetProcessSeq();
	CProcMgr& procMgr = Singleton<CProcMgr>::getInstance();

	CRefPtr<CProcess> pProcess = procMgr.RefProcessBySeq(Seq);
	if (pProcess.IsNull()) {
		return TRUE;
	}else{
		return pProcess->IsMarkExit();
	}
}

LARGE_INTEGER CEventView::GetProcessExitTime()
{
	LARGE_INTEGER ExitTime;
	ULONG Seq = m_EventView.GetProcessSeq();
	CProcMgr& procMgr = Singleton<CProcMgr>::getInstance();

	ExitTime.QuadPart = 0;

	CRefPtr<CProcess> pProcess = procMgr.RefProcessBySeq(Seq);
	if (!pProcess.IsNull()) {
		CRefPtr<CLogEvent> pExitEvent = pProcess->GetExitEvent();
		if (!pExitEvent.IsNull()) {
			CBaseView baseView(pExitEvent);
			return baseView.GetStartTime();
		}
	}

	return ExitTime;
}

BOOL CEventView::IsProcessFromInit()
{
	return m_ProcView.GetOperator() == NOTIFY_PROCESS_INIT;
}

CString
CEventView::GetOperationStrResult(
	_In_ MAP_SOURCE_TYPE SrcType
)
{
	CString strSrc;
	switch (SrcType)
	{
	case emArchiteture:
		strSrc = IsWow64() ? TEXT("32-bit") : TEXT("64-bit");
		break;
	case emAuthId:
		LUID AuthId = GetAuthId();
		strSrc.Format(TEXT("%08x:%08x"), AuthId.HighPart, AuthId.LowPart);
		break;
	case emCategory:
		break;
	case emCommandLine:
		strSrc = GetCommandLine();
		break;
	case emCompany:
		strSrc = GetCompanyName();
		break;
	case emCompletionTime:
		strSrc = UtilConvertTimeOfDay(GetCompleteTime());
		break;
	case emDataTime:
		strSrc = UtilConvertDay(GetStartTime());
		break;
	case emDescription:
		strSrc = GetDisplayName();
		break;
	case emDetail:
		strSrc = GetDetail();
		break;
	case emDuration:

		//
		// TODO
		//

		break;
	case emEventClass:
		strSrc = StrMapClassEvent(GetEventClass());
		break;
	case emImagePath:
		strSrc = GetImagePath();
		break;
	case emIntegrity:
		strSrc = StrMapIntegrityLevel(GetIntegrity());
		break;
	case emOperation:
		strSrc = StrMapOperation(GetPreEventEntry());
		break;
	case emParentPid:
		strSrc.Format(TEXT("%d"), GetParentProcessId());
		break;
	case emPath:
		strSrc = GetPath();
		break;
	case emPID:
		strSrc.Format(TEXT("%d"), GetProcessId());
		break;
	case emProcessName:
		strSrc = GetProcessName();
		break;
	case emRelativeTime:

		//
		// TODO
		//

		break;
	case emResult:
		strSrc = StrMapNtStatus(GetResult());
		break;
	case emSequence:
		strSrc.Format(TEXT("%lu"), GetSeqNumber());
		break;
	case emSession:
		strSrc.Format(TEXT("%u"), GetSessionId());
		break;
	case emTID:
		strSrc.Format(TEXT("%d"), GetThreadId());
		break;
	case emTimeOfDay:
		strSrc = UtilConvertTimeOfDay(GetStartTime());
		break;
	case emUser:
		strSrc = StrMapUserNameFromSid(GetUserSid());
		break;
	case emVersion:
		strSrc = GetVersion();
		break;
	case emVirtualize:
		strSrc = IsVirtualize() ? TEXT("True") : TEXT("False");
	default:
		break;
	}

	return strSrc;
}

