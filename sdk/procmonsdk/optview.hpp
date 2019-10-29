#pragma once

#include <vector>
#include "refobject.hpp"
#include "module.hpp"
#include "operator.hpp"
#include "viewer.hpp"

class COptView : public CRefBase
{

public:
	COptView();
	~COptView();

public:

	void SetEventOpt(CRefPtr<COperator> Opt);
	void SnapProcess(CRefPtr<CProcess> Opt);

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

	//
	// for Process
	//
	
	DWORD GetProcessId();
	DWORD GetSessionId();
	DWORD GetThreadId();
	DWORD GetParentProcessId();
	LUID GetAuthId();
	CString GetUserName();
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


private:
	//CRefPtr<COperator> m_EventOpt;
	//CRefPtr<COperator> m_ProcOpt;

	CProcCreateInfoView m_ProcView;
	CBaseView m_EventView;

	std::vector<CModule> m_ModuleInfo;
	CRefPtr<CProcInfo> m_ProcInfo;
};