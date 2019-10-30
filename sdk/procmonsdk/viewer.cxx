
#include "pch.hpp"
#include "event.hpp"
#include "viewer.hpp"

CProcCreateInfoView::CProcCreateInfoView(CRefPtr<CLogEvent> Opt) :
	CBaseView(Opt)
{

}

CProcCreateInfoView::CProcCreateInfoView()
{

}

CProcCreateInfoView::~CProcCreateInfoView()
{

}

BOOL CProcCreateInfoView::IsValid()
{
	if (CBaseView::IsValid()){
		USHORT Type = m_Event->GetNotifyType();
		if (Type == NOTIFY_PROCESS_CREATE ||
			Type == NOTIFY_PROCESS_INIT) {
			return TRUE;
		}
	}
	return FALSE;
}

DWORD CProcCreateInfoView::GetProcSeq()
{
	return GetProcCreateInfo()->Seq;
}

DWORD CProcCreateInfoView::GetProcessId()
{
	return GetProcCreateInfo()->ProcessId;
}

DWORD CProcCreateInfoView::GetSessionId()
{
	return GetProcCreateInfo()->SessionId;
}

DWORD CProcCreateInfoView::GetParentProcessId()
{
	return GetProcCreateInfo()->ParentId;
}

LUID CProcCreateInfoView::GetAuthId()
{
	return GetProcCreateInfo()->AuthenticationId;
}

CString CProcCreateInfoView::GetUserName()
{
	return TEXT("");
}

DWORD CProcCreateInfoView::GetIntegrity()
{
	return 0;
}

BOOL CProcCreateInfoView::IsVirtualize()
{
	return GetProcCreateInfo()->TokenVirtualizationEnabled != 0;
}

BOOL CProcCreateInfoView::IsWow64()
{
	return !GetProcCreateInfo()->IsWow64;
}

CString CProcCreateInfoView::GetProcessName()
{
	return CString(PathFindFileName(GetImagePath()));
}

CString CProcCreateInfoView::GetImagePath()
{
	CString strProcessName;
	PLOG_PROCESSCREATE_INFO pCreateInfo = GetProcCreateInfo();

	if (pCreateInfo && pCreateInfo->ProcNameLength) {
		PUCHAR pBufferEnd = (PUCHAR)(pCreateInfo + 1);
		CString strImagePath;

		pBufferEnd += pCreateInfo->SidLength;
		pBufferEnd += pCreateInfo->IntegrityLevelSidLength;

		strImagePath.Append((LPCWSTR)pBufferEnd, pCreateInfo->ProcNameLength);

		//
		// Convert to dos path
		//

		UtilConvertNtInternalPathToDosPath(strImagePath, strProcessName);


	}
	return strProcessName;
}

CString CProcCreateInfoView::GetCommandLine()
{
	CString strCmdline;
	PLOG_PROCESSCREATE_INFO pCreateInfo = GetProcCreateInfo();

	if (pCreateInfo && pCreateInfo->CommandLineLength) {
		PUCHAR pBufferEnd = (PUCHAR)(pCreateInfo + 1);
		pBufferEnd += pCreateInfo->SidLength;
		pBufferEnd += pCreateInfo->IntegrityLevelSidLength;
		pBufferEnd += pCreateInfo->ProcNameLength * sizeof(WCHAR);
		strCmdline.Append((LPCWSTR)pBufferEnd, pCreateInfo->CommandLineLength);
	}
	return strCmdline;
}

FORCEINLINE
PLOG_PROCESSCREATE_INFO 
CProcCreateInfoView::GetProcCreateInfo()
{
	if (m_Event->getPreLog().GetBufferLen()){
		PLOG_ENTRY pEntry = reinterpret_cast<PLOG_ENTRY>(m_Event->getPreLog().GetBuffer());
		return TO_EVENT_DATA(PLOG_PROCESSCREATE_INFO, pEntry);
	}

	return NULL;
}

BOOL CBaseView::IsValid()
{
	if (!m_Event.IsNull()){
		if (m_Event->getPreLog().GetBufferLen()){
			return TRUE;
		}
	}
	return FALSE;
}

DWORD CBaseView::GetThreadId()
{
	return GetPreLogEntry()->ThreadId;
}

DWORD CBaseView::GetSeqNumber()
{
	return m_Event->GetSeq();
}

DWORD CBaseView::GetEventClass()
{
	return m_Event->GetMoniterType();
}

DWORD CBaseView::GetOperator()
{
	return m_Event->GetNotifyType();
}

LARGE_INTEGER CBaseView::GetStartTime()
{
	return GetPreLogEntry()->Time;
}

ULONG CBaseView::GetStackFrameCount()
{
	return GetPreLogEntry()->nFrameChainCounts;
}

LARGE_INTEGER CBaseView::GetCompleteTime()
{
	auto pEntry = GetPostLogEntry();
	return pEntry ? pEntry->Time : GetPreLogEntry()->Time;
}


CString CBaseView::GetPath()
{
	CString strDosPath;
	if (!m_Event->GetPath().IsEmpty()){
		UtilConvertNtInternalPathToDosPath(m_Event->GetPath(), strDosPath);
	}
	return strDosPath;
}

CString CBaseView::GetDetail()
{
	return m_Event->GetDetail();
}

NTSTATUS CBaseView::GetResult()
{
	auto pEntry = GetPostLogEntry();
	return pEntry ? pEntry->Status : GetPreLogEntry()->Status;
}

DWORD CBaseView::GetProcessSeq()
{
	return GetPreLogEntry()->ProcessSeq;
}

CRefPtr<CLogEvent> CBaseView::GetEvent()
{
	return m_Event;
}

DWORD CBaseView::GetCallStack(std::vector<PVOID>& callStacks)
{
	PLOG_ENTRY pEntry = GetPreLogEntry();
	
	if (pEntry && pEntry->nFrameChainCounts){

		PVOID* pCallback = (PVOID*)(pEntry + 1);
		for (int i = 0; i < pEntry->nFrameChainCounts; i++)
		{
			callStacks.push_back(pCallback[i]);
		}
	}

	return (DWORD)callStacks.size();
}

void CBaseView::SetEvent(CRefPtr<CLogEvent> pEvent)
{
	m_Event = pEvent;
}

FORCEINLINE
PLOG_ENTRY 
CBaseView::GetPreLogEntry()
{
	return reinterpret_cast<PLOG_ENTRY>
		(m_Event->getPreLog().GetBuffer());
}

FORCEINLINE
PLOG_ENTRY 
CBaseView::GetPostLogEntry()
{
	return reinterpret_cast<PLOG_ENTRY>
		(m_Event->getPostLog().GetBuffer());
}
