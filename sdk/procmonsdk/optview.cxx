
#include "pch.hpp"
#include "optview.hpp"
#include "process.hpp"
#include "utils.hpp"


COptView::COptView()
{

}

COptView::~COptView()
{

}

void COptView::SetEventOpt(CRefPtr<COperator> Opt)
{
	m_EventView = Opt;
}

void COptView::SnapProcess(CRefPtr<CProcess> pProcess)
{
	m_ProcView = pProcess->GetOpt();
	m_ModuleInfo = pProcess->GetModuleList();
	m_ProcInfo = pProcess->GetProcInfo();
}

DWORD COptView::GetSeqNumber()
{
	return m_EventView.GetSeqNumber();
}

DWORD COptView::GetEventClass()
{
	return m_EventView.GetEventClass();
}

DWORD COptView::GetEventOperator()
{
	return m_EventView.GetOperator();
}

LARGE_INTEGER COptView::GetStartTime()
{

	return m_EventView.GetStartTime();
}

LARGE_INTEGER COptView::GetCompleteTime()
{
	return m_EventView.GetCompleteTime();
}

CString COptView::GetPath()
{
	return m_EventView.GetPath();
}

CString COptView::GetDetail()
{
	return m_EventView.GetDetail();
}

NTSTATUS COptView::GetResult()
{
	return m_EventView.GetResult();
}

DWORD COptView::GetCallStack(std::vector<PVOID>& callStacks)
{
	return m_EventView.GetCallStack(callStacks);
}

DWORD COptView::GetProcessSeq()
{
	return m_EventView.GetProcessSeq();
}

PLOG_ENTRY COptView::GetPreEventEntry()
{
	if (!m_EventView.GetOpt().IsNull()){
		if (m_EventView.GetOpt()->getPreLog().GetBufferLen()){
			return reinterpret_cast<PLOG_ENTRY>(m_EventView.GetOpt()->getPreLog().GetBuffer());
		}
	}
	return NULL;
	
}

DWORD COptView::GetProcessId()
{
	return m_ProcView.GetProcessId();
}

DWORD COptView::GetSessionId()
{
	return m_ProcView.GetSessionId();
}

DWORD COptView::GetThreadId()
{
	return m_EventView.GetThreadId();
}

DWORD COptView::GetParentProcessId()
{
	return m_ProcView.GetParentProcessId();
}

LUID COptView::GetAuthId()
{
	return m_ProcView.GetAuthId();
}

CString COptView::GetUserName()
{
	return m_ProcView.GetUserName();
}

DWORD COptView::GetIntegrity()
{
	return m_ProcView.GetIntegrity();
}

BOOL COptView::IsVirtualize()
{
	return m_ProcView.IsVirtualize();
}

CString COptView::GetProcessName()
{
	return m_ProcView.GetProcessName();
}

CString COptView::GetImagePath()
{
	return m_ProcView.GetImagePath();
}

CString COptView::GetCommandLine()
{
	return m_ProcView.GetCommandLine();
}

BOOL COptView::IsWow64()
{
	return m_ProcView.IsWow64();
}

CBuffer& COptView::GetProcIcon(BOOL bSmall)
{
	if (bSmall) {
		return m_ProcInfo->GetSmallIcon();
	}else{
		return m_ProcInfo->GetLargeIcon();
	}
}

const CString& COptView::GetCompanyName()
{
	return m_ProcInfo->GetCompanyName();
}

const CString& COptView::GetDisplayName()
{
	return m_ProcInfo->GetDisplayName();
}

const CString& COptView::GetVersion()
{
	return m_ProcInfo->GetVersion();
}

std::vector<CModule>& COptView::GetModuleList()
{
	return m_ModuleInfo;
}


