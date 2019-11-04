# Openprocmon
open source process monitor

## How to use

1. Use the procmon gui. (build and run procmon_gui.exe)
2. Use the sdk in you project(build and link sdk)

You don't have a digital signature yourself? It doesn't matter. You can use the original procmon driver, this sdk is 100% compatible with the original procmon driver.

## SDK example

```cpp
#include <conio.h>
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
		return TRUE;
	}
};


int main()
{

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
	return 0;
}
```

It is pertty esay right?

## GUI Snapshot

main window:

![main_window](https://github.com/progmboy/openprocmon/blob/master/images/mian_gui.png)

properties windows

![prop_event](https://github.com/progmboy/openprocmon/blob/master/images/prop_event.png)
![prop_proc](https://github.com/progmboy/openprocmon/blob/master/images/prop_proc.png)
![prop_stack](https://github.com/progmboy/openprocmon/blob/master/images/prop_stack.png)

## TODO

### Example
- [ ] Driver load example.

### GUI

- [x] Filter dialog.
- [x] Filter apply processing dialog.
- [ ] Save the capture log to file.
- [ ] Load capture log.
- [x] Load Driver.
- [ ] Sybmol support for call stack view.
- [x] Integrity level parse.
- [ ] Registery event capture.
- [ ] Parse detail for File/Registery Event.
- [ ] Filter plugin support.
