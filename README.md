# Openprocmon
open source process monitor

## Menu

- [How to use](#how-to-use)
- [How to build](#how-to-build)
    - [Prepare the environment](#prepare-the-environment)
    - [Visual Studio](#visual-studio)
    - [CMake](#cmake)
- [SDK example](#sdk-example)
- [GUI Snapshot](#gui-snapshot)
- [How to show stackframe with symbol](#how-to-show-stackframe-with-symbol)
- [About another branch](#about-another-branch)
- [TODO](#todo)
    - [GUI](#gui)
    

## How to use

1. Use the procmon gui. (build and run procmon_gui.exe)
2. Use the sdk in you project(build and link sdk)
3. Hack the driver to implement your own EDR or something.

You don't have a digital signature yourself? It doesn't matter. You can use the original procmon driver, this sdk is 100% compatible with the original procmon driver.
And of course, The original procmon driver can be replaced with this driver to learn how procmon works.

## How to build

### Prepare the environment

**WDK**

Install the last [WDK](https://docs.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk)

**WTL**

Download the last [WTL library](https://sourceforge.net/projects/wtl/) and put it in folder whatever you like. for example i put it in "D:\source\WTL10_9163"

### Visual Studio

1. Open procmon.sln use visual studio
2. change the addtion include directoy of procmon_gui from "D:\source\WTL10_9163\Include" to yours
3. build.
4. sign the driver or disable driver signature enforcement.
5. run.

### CMake

1. Install CMake.
2. Run cmake to generate the project
```
cmake .. -G "Visual Studio 16 2019" -A X64 -DWTL_ROOT_DIR=D:\source\WTL10_9163 -DWDK_WINVER=0x0A00
```
3. build
```
cmake --build . --config Release
```
4. sign the driver or disable driver signature enforcement.

**!!!Please note that I don't how to use the cmake to sign the driver with test signature. please do it yourself!!**

5. run


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
	return 0;
}

```

It is pertty esay right?

## GUI Snapshot

**The GUI is still in Pre-Alpha state, and many features have yet to be improved. Wellcome PR.**

main window:

![main_window](https://github.com/progmboy/openprocmon/blob/master/images/mian_gui.png)

properties windows

![prop_event](https://github.com/progmboy/openprocmon/blob/master/images/prop_event.png)
![prop_proc](https://github.com/progmboy/openprocmon/blob/master/images/prop_proc.png)
![prop_stack](https://github.com/progmboy/openprocmon/blob/master/images/prop_stack.png)

## How to show stackframe with symbol

1. Go to windbg.exe directory copy the following files to the same directory with "procmon_gui.exe".
```
dbghelp.dll
symsrv.dll
symsrv.yes
```
2. Set the _NT_SYMBOL_PATH environment variable. for example:
```
srv*D:\reverse\symbols*https://msdl.microsoft.com/download/symbols
```

## About another branch
Discover it yourself!!!

## TODO
### GUI

- [x] ~~Filter dialog.~~
- [x] ~~Filter apply processing dialog.~~
- [ ] Save the capture log to file.
- [ ] Load capture log.
- [x] ~~Load Driver.~~
- [x] ~~Sybmol support for call stack view.~~
- [x] ~~Integrity level parse.~~
- [ ] Open registery event capture.
- [ ] Parse detail for File/Registery Event.
- [ ] Filter plugin support.
- [ ] Main menu message.
- [x] ~~Highlight support.~~
- [x] ~~filter mechanism~~
