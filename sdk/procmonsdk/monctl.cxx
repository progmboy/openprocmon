
#include "pch.hpp"
#include "logger.hpp"
#include "eventmgr.hpp"
#include "monctl.hpp"

#include <assert.h>

#pragma comment(lib, "FltLib.lib")

CMonitorContoller::CMonitorContoller()
{

}

CMonitorContoller::~CMonitorContoller()
{
	DisConnect();
}


BOOL CMonitorContoller::Connect()
{
	BOOL bOk = FALSE;
	ULONG Flag = 0;
	int i = 2;

	while (i--)
	{
		HRESULT hResult = FilterConnectCommunicationPort(PROCMON_PORTNAME,
			0,
			&Flag,
			sizeof(ULONG),
			NULL,
			&m_hPort);

		if (IS_ERROR(hResult)) {

			//
			// Try to load driver
			//

			if (hResult == HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)){
				CDrvLoader& drvLoader = Singleton<CDrvLoader>::getInstance();
				if (drvLoader.IsReady() && drvLoader.Load()) {
					continue;
				}else{
					LogMessage(L_ERROR, TEXT("Could not connect to filter: 0x%08x"), hResult);
					return FALSE;
				}
			}else{
				break;
			}

		}else{
			bOk = TRUE;
			break;
		}
	}

	if (bOk){
		m_RecvThread.Init(m_hPort);
	}
	
	return bOk;
}

VOID 
CMonitorContoller::DisConnect()
{
	if (m_hPort){
		CloseHandle(m_hPort);
		m_hPort = NULL;
	}
}

VOID 
CMonitorContoller::SetMonitor(
	IN BOOL bEnableProc, 
	IN BOOL bEnableFile, 
	IN BOOL bEnableReg
)
{
	m_dwControl = 0;
	if (bEnableProc){
		m_dwControl |= CTL_MONITOR_PROC_ON;
	}

	if (bEnableFile) {
		m_dwControl |= CTL_MONITOR_FILE_ON;
	}

	if (bEnableReg) {
		m_dwControl |= CTL_MONITOR_REG_ON;
	}
}

BOOL CMonitorContoller::DisableAll()
{
	return Control(CTL_MONITOR_ALL_CLOSE);
}

BOOL CMonitorContoller::Start()
{
	bool bRet;

	//
	// start processing thread
	//
	
	bRet = m_OptThread.Start();
	if (!bRet){
		return FALSE;
	}
	
	//
	// start event receive thread
	//
	
	bRet = m_RecvThread.Start();
	if (!bRet){
		m_OptThread.Stop();
		return FALSE;
	}
	
	return Control(m_dwControl);
}

BOOL CMonitorContoller::Stop()
{
	BOOL bRet;
	bRet = DisableAll();
	EVENTMGR().Clear();
	//PROCMGR().Clear();

	return bRet;
}

BOOL CMonitorContoller::Destory()
{
	DisableAll();
	m_OptThread.Stop();
	m_RecvThread.Stop();
	DisConnect();
	return TRUE;
}

BOOL
CMonitorContoller::Control()
{
	return Control(m_dwControl);
}

BOOL 
CMonitorContoller::Control(
	IN DWORD Flags
)
{
	HRESULT hResult;
	DWORD dwRetBytes;
	FLTMSG_CONTROL_FLAGS Controls;

	Controls.Head.CtlCode = 0;
	Controls.Flags = Flags;
	hResult = FilterSendMessage(m_hPort, &Controls, sizeof(Controls), NULL, 0, &dwRetBytes);
	if (hResult != S_OK) {
		LogMessage(L_ERROR, TEXT("Can not enable monitor"));
		return FALSE;
	}
	return TRUE;
}

BOOL CRecvThread::Init(HANDLE hPort)
{
	m_hPort = hPort;
	return TRUE;
}



void CRecvThread::Run()
{
	HRESULT hResult;
	ULONG MessageLength = MAX_PROCMON_MESSAGE_LEN + sizeof(PROCMON_MESSAGE_HEADER);
	PPROCMON_MESSAGE_HEADER pMessage = (PPROCMON_MESSAGE_HEADER)HeapAlloc(GetProcessHeap(), 0,
		MAX_PROCMON_MESSAGE_LEN + sizeof(PROCMON_MESSAGE_HEADER));
	OVERLAPPED Overlapped = { 0 };

	if (!pMessage) {
		return;
	}

	ZeroMemory(&Overlapped, sizeof(Overlapped));
	Overlapped.hEvent = CreateEvent(NULL, FALSE, FALSE, NULL);
	if (!Overlapped.hEvent) {
		HeapFree(GetProcessHeap(), 0, pMessage);
		return;
	}

	while (!IsStop())
	{
		hResult = FilterGetMessage(m_hPort,
			&pMessage->Header, MessageLength, &Overlapped);

		if (hResult == HRESULT_FROM_WIN32(ERROR_IO_PENDING)) {

			//if(WaitForSingleObject(Overlapped.hEvent, 500) == WAIT_TIMEOUT){
			//	continue;
			//}
			
			//
			// wait until data ready
			//
			
			BOOL bNeedBreak = FALSE;
			while (TRUE)
			{
				DWORD dwWaitResult = WaitForSingleObject(Overlapped.hEvent, 500);
				if (dwWaitResult == WAIT_TIMEOUT){
					if (IsStop()){
						bNeedBreak = TRUE;
						break;
					}else{
						
						//
						// Continue waiting
						//
						
					}
				}else if (dwWaitResult == WAIT_OBJECT_0){
					
					//
					// the data ready
					//
					
					break;
				}else{
					bNeedBreak = TRUE;
					break;
				}
			}

			if (bNeedBreak) {
				break;
			}


		}else if (hResult != S_OK) {

			//
			// TODO error
			//

			assert(FALSE);

		}

		//
		// here we receive a message block.
		// try to decode the block
		//

		PLOG_ENTRY pEntries = (PLOG_ENTRY)(pMessage + 1);
		
		//
		// pass to operator mgr
		//
		
		if(!Singleton<CEventMgr>::getInstance().ProcessMsgBlocks(pEntries, pMessage->Length)){
			LogMessage(L_WARN, TEXT("Failed to process msg blocks"));
		}

	}

	LogMessage(L_INFO, TEXT(".......recv thread exit....."));

	//
	// clean up
	//

	if (pMessage) {
		HeapFree(GetProcessHeap(), 0, pMessage);
	}
}

void COPtThread::Run()
{
	while (!IsStop())
	{
		
		//
		// If queue have no data, the function return false
		// for every 500ms. so here we have a opportunity to 
		// exit the loop
		//
		
		Singleton<CEventMgr>::getInstance().Process();
	}

	LogMessage(L_INFO, TEXT(".......processing thread exit....."));

}

