#pragma once

#include "refobject.hpp"
#include "event.hpp"
#include <vector>

class CBaseView
{
public:
	CBaseView(CRefPtr<CLogEvent> pEvent)
	{
		m_Event = pEvent;
	}

	CBaseView()
	{

	}

	~CBaseView()
	{

	}

	virtual BOOL IsValid();


	DWORD GetThreadId();
	DWORD GetSeqNumber();
	DWORD GetEventClass();
	DWORD GetOperator();
	LARGE_INTEGER GetStartTime();
	ULONG GetStackFrameCount();
	LARGE_INTEGER GetCompleteTime();
	CString GetPath();
	CString GetDetail();
	NTSTATUS GetResult();
	DWORD GetProcessSeq();
	CRefPtr<CLogEvent> GetEvent();
	DWORD GetCallStack(std::vector<PVOID>& callStacks);

	void SetEvent(CRefPtr<CLogEvent> pEvent);

protected:

	PLOG_ENTRY GetPreLogEntry();
	PLOG_ENTRY GetPostLogEntry();

protected:
	CRefPtr<CLogEvent> m_Event;
};

class CProcInfoView : public CBaseView
{
public:
	CProcInfoView();
	CProcInfoView(CRefPtr<CLogEvent> Opt);
	~CProcInfoView();

public:
	virtual BOOL IsValid();

	DWORD GetProcSeq();
	DWORD GetProcessId();
	DWORD GetSessionId();
	DWORD GetParentProcessId();
	LUID GetAuthId();
	CString GetUserName();
	DWORD GetIntegrity();
	BOOL IsVirtualize();
	BOOL IsWow64();

	CString GetProcessName();
	CString GetImagePath();
	CString GetCommandLine();

	PSID GetUserSid();

private:
	PLOG_PROCESSCREATE_INFO GetProcCreateInfo();
};