// sdktest.cpp : This file contains the 'main' function. Program execution begins and ends there.
//


#include <conio.h>
#include "../../sdk/procmonsdk/sdk.hpp"
#include <atltime.h>

class CEvent : public IEventCallback
{
	
public:
	virtual BOOL DoEvent(const CRefPtr<COptView> pEventView)
	{

		ULONGLONG Time = pEventView->GetStartTime().QuadPart;

		LogMessage(L_INFO, TEXT("%llu Process %s Do 0x%x for %s"),
			Time,
			pEventView->GetProcessName().GetBuffer(),
			pEventView->GetEventOperator(),
			pEventView->GetPath().GetBuffer());
		return TRUE;
	}
};

UINT GetSize(UINT Size, UINT Align)
{
	return  (UINT)ceil(Size / (float)Align) * Align;
}



int main()
{

#if 0
	//_getch();
	COperatorMgr& Optmgr = Singleton<COperatorMgr>::getInstance();
	CMonitorContoller& Monitormgr = Singleton<CMonitorContoller>::getInstance();
	
	Optmgr.RegisterCallback(new CEvent);

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

	//_getch();
	
	//
	// try to stop the monitor
	//
	
	//Monitormgr.Stop();

	//LogMessage(L_INFO, TEXT("!!!!!monitor stop press any key to start!!!!"));
	//_getch();

	//Monitormgr.Start();

	_getch();

	Monitormgr.Stop();
	Monitormgr.Destory();
#endif

	CString strFile = TEXT("C:\\windows\\system32\\notepad.exe");

// 	CString strVer;
// 	CString strCompany;
// 	CString strDesc;
// 	if(UtilGetFileVersionInfo(strFile, strDesc, strCompany, strVer)){
// 		LogMessage(L_INFO, TEXT("\"%s\" \"%s\" \"%s\""), strDesc.GetBuffer(), strCompany.GetBuffer(), strVer.GetBuffer());
// 	}else{
// 		LogMessage(L_INFO, TEXT("Error"));
// 	}
	
	//CBuffer bufSmall;
	//CBuffer bufBig;
	//UtilExtractIcon(strFile, bufSmall, bufBig);

	//UINT nNewSize = (UINT)ceil(nRequestedSize / 1024.0) * 1024;


	return 0;
}
