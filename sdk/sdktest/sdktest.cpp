// sdktest.cpp : This file contains the 'main' function. Program execution begins and ends there.
//


#include <conio.h>
#include <atltime.h>
#include "../../sdk/procmonsdk/sdk.hpp"

std::vector<CRefPtr<CEventView>> m_viewList;

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

UINT GetSize(UINT Size, UINT Align)
{
	return  (UINT)ceil(Size / (float)Align) * Align;
}



int main()
{

#if 1
	CEventMgr& Optmgr = Singleton<CEventMgr>::getInstance();
	CMonitorContoller& Monitormgr = Singleton<CMonitorContoller>::getInstance();
	
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
	return 0;
}
