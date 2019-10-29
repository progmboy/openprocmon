#pragma once

class CBaseView
{
public:
	CBaseView(CRefPtr<COperator> Opt)
	{
		m_Opt = Opt;
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
	CRefPtr<COperator> GetOpt();
	DWORD GetCallStack(std::vector<PVOID>& callStacks);

	void SetOpt(CRefPtr<COperator> Opt);

protected:

	PLOG_ENTRY GetPreLogEntry();
	PLOG_ENTRY GetPostLogEntry();

protected:
	CRefPtr<COperator> m_Opt;
};

class CProcCreateInfoView : public CBaseView
{
public:
	CProcCreateInfoView();
	CProcCreateInfoView(CRefPtr<COperator> Opt);
	~CProcCreateInfoView();

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

private:
	PLOG_PROCESSCREATE_INFO GetProcCreateInfo();
};