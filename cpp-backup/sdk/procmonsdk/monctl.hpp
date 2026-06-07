#pragma once


#define MONITORMGR() Singleton<CMonitorContoller>::getInstance()

class CRecvThread : public CThread
{
public:
	CRecvThread() {};
	~CRecvThread() {};

public:

	BOOL Init(HANDLE hPort);
	virtual void Run();

private:
	HANDLE m_hPort = NULL;
};

class COPtThread : public CThread
{
public:
	COPtThread() {};
	~COPtThread() {};

public:
	virtual void Run();
};


class CMonitorContoller
{
public:
	CMonitorContoller();
	virtual ~CMonitorContoller();

public:
	
	BOOL Connect();
	VOID DisConnect();
	VOID SetMonitor(
		IN BOOL bEnableProc, 
		IN BOOL bEnableFile, 
		IN BOOL bEnableReg);
	BOOL DisableAll();
	BOOL Start();
	BOOL Stop();
	BOOL Destory();
	
private:
	BOOL Control(IN DWORD Flags);
	BOOL Control();

private:

	HANDLE m_hPort = NULL;
	COPtThread m_OptThread;
	CRecvThread m_RecvThread;
	DWORD m_dwControl = 0;
};