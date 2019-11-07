#pragma once

#include "event.hpp"
#include "process.hpp"
#include "procmgr.hpp"

typedef enum _MAP_SOURCE_TYPE
{
	emArchiteture,
	emAuthId,
	emCategory,
	emCommandLine,
	emCompany,
	emCompletionTime,
	emDataTime,
	emDescription,
	emDetail,
	emDuration,
	emEventClass,
	emImagePath,
	emIntegrity,
	emOperation,
	emParentPid,
	emPath,
	emPID,
	emProcessName,
	emRelativeTime,
	emResult,
	emSequence,
	emSession,
	emTID,
	emTimeOfDay,
	emUser,
	emVersion,
	emVirtualize,
	emInvalid,
}MAP_SOURCE_TYPE;

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
	PLOG_ENTRY GetPostEventEntry();

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
	CString GetOperationStrResult(_In_ MAP_SOURCE_TYPE SrcType);


private:

	CProcInfoView m_ProcView;
	CBaseView m_EventView;

	std::vector<CModule> m_ModuleInfo;
	CRefPtr<CProcInfo> m_ProcInfo;
};