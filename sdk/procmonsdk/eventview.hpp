#pragma once

#include "event.hpp"
#include "process.hpp"
#include "procmgr.hpp"

class CEventView : public CRefBase
{

public:
	CEventView();
	~CEventView();

public:

	void SetEventOpt(CRefPtr<CLogEvent> pEvent);
	void SnapProcess(CRefPtr<CProcess> pProcess);

public:
	
	BOOL IsReady();

	//
	// For Event
	//
	
	DWORD GetSeqNumber();
	DWORD GetEventClass();
	DWORD GetEventOperator();
	LARGE_INTEGER GetStartTime();
	LARGE_INTEGER GetCompleteTime();
	CString GetPath();
	CString GetDetail();
	NTSTATUS GetResult();
	DWORD GetCallStack(std::vector<PVOID>& callStacks);
	DWORD GetProcessSeq();
	PLOG_ENTRY GetPreEventEntry();
	BOOL IsImpersonate();
	BOOL IsImpersonateOpen();

	//
	// for Process
	//
	
	DWORD GetProcessId();
	DWORD GetSessionId();
	DWORD GetThreadId();
	DWORD GetParentProcessId();
	LUID GetAuthId();
	CString GetUserName();
	PSID GetUserSid();
	DWORD GetIntegrity();
	BOOL IsVirtualize();
	CString GetProcessName();
	CString GetImagePath();
	CString GetCommandLine();
	BOOL IsWow64();

	CBuffer& GetProcIcon(BOOL bSmall = TRUE);
	const CString& GetCompanyName();
	const CString& GetDisplayName();
	const CString& GetVersion();

	std::vector<CModule>& GetModuleList();

	BOOL IsProcessExit();
	LARGE_INTEGER GetProcessExitTime();
	BOOL IsProcessFromInit();


private:

	CProcInfoView m_ProcView;
	CBaseView m_EventView;

	std::vector<CModule> m_ModuleInfo;
	CRefPtr<CProcInfo> m_ProcInfo;
};