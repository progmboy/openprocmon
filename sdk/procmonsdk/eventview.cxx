
#include "pch.hpp"
#include "eventview.hpp"
#include "process.hpp"
#include "utils.hpp"


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

BOOL CEventView::IsImpersonate()
{
	return m_EventView.IsImpersonate();
}

BOOL CEventView::IsImpersonateOpen()
{
	return m_EventView.IsImpersonateOpen();
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


