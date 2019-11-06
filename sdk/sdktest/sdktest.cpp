// sdktest.cpp : This file contains the 'main' function. Program execution begins and ends there.
//


#include <conio.h>
#include <atltime.h>
#include "../../sdk/procmonsdk/sdk.hpp"

class CMyEvent : public IEventCallback
{
public:
	virtual BOOL DoEvent(const CRefPtr<CEventView> pEventView)
	{

		ULONGLONG Time = pEventView->GetStartTime().QuadPart;

		LogMessage(L_INFO, TEXT("%llu Process %s Do 0x%x for %s"),
			Time,
			pEventView->GetProcessName().GetBuffer(),
			pEventView->GetEventOperator(),
			pEventView->GetPath().GetBuffer());
		//m_viewList.push_back(pEventView);
		return TRUE;
	}
};


int main()
{
#if 0
	CEventMgr& Optmgr = Singleton<CEventMgr>::getInstance();
	CMonitorContoller& Monitormgr = Singleton<CMonitorContoller>::getInstance();
	CDrvLoader& Drvload = Singleton<CDrvLoader>::getInstance();
	
	if(!Drvload.Init(TEXT("PROCMON24"), TEXT("procmon.sys"))){
		return -1;
	}
	Optmgr.RegisterCallback(new CMyEvent);

	//
	// Try to connect to procmon driver
	//
	
	if (!Monitormgr.Connect()){
		LogMessage(L_ERROR, TEXT("Cannot connect to procmon driver"));
		return -1;
	}
	
	//
	// try to start monitor
	//
	
	Monitormgr.SetMonitor(TRUE, TRUE, FALSE);
	if (!Monitormgr.Start()){
		LogMessage(L_ERROR, TEXT("Cannot start the mointor"));
		return -1;
	}

	_getch();
	
	//
	// try to stop the monitor
	//
	
	Monitormgr.Stop();

	LogMessage(L_INFO, TEXT("!!!!!monitor stop press any key to start!!!!"));
	_getch();

	Monitormgr.Start();

	_getch();

	Monitormgr.Stop();
	Monitormgr.Destory();

#endif

	LogMessage(L_INFO, TEXT("(0x%x) %s"), 0x10080, (LPCTSTR)StrMapFileAccessMask(0x10080));
	LogMessage(L_INFO, TEXT("(0x%x) %s"), FILE_ALL_ACCESS, (LPCTSTR)StrMapFileAccessMask(FILE_ALL_ACCESS));
	LogMessage(L_INFO, TEXT("(0x%x) %s"), FILE_GENERIC_WRITE, (LPCTSTR)StrMapFileAccessMask(FILE_GENERIC_WRITE));
	LogMessage(L_INFO, TEXT("(0x%x) %s"), FILE_READ_DATA | FILE_READ_EA, (LPCTSTR)StrMapFileAccessMask(FILE_READ_DATA | FILE_READ_EA));
	LogMessage(L_INFO, TEXT("(0x%x) %s"), FILE_GENERIC_READ, (LPCTSTR)StrMapFileAccessMask(FILE_GENERIC_READ));

	return 0;
}
