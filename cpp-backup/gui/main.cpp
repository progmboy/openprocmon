// procmon_gui.cpp : main source file for procmon_gui.exe
//

#include "stdafx.h"

#include <shellscalingapi.h>

#include "resource.h"

#include "View.h"
#include "aboutdlg.h"
#include "propdlg.h"
#include "propproc.h"
#include "propstack.h"
#include "filter.hpp"
#include "fltprocess.h"
#include "filterdlg.h"
#include "MainFrm.h"


#pragma comment(lib, "Shcore.lib")

CAppModule _Module;

int Run(LPTSTR /*lpstrCmdLine*/ = NULL, int nCmdShow = SW_SHOWDEFAULT)
{
	CMessageLoop theLoop;
	_Module.AddMessageLoop(&theLoop);

	CRefPtr<CMainFrame> wndMain = new CMainFrame;

	if(wndMain->CreateEx() == NULL)
	{
		ATLTRACE(_T("Main window creation failed!\n"));
		return 0;
	}

	wndMain->ShowWindow(nCmdShow);

	int nRet = theLoop.Run();

	_Module.RemoveMessageLoop();
	return nRet;
}

int WINAPI _tWinMain(HINSTANCE hInstance, HINSTANCE /*hPrevInstance*/, LPTSTR lpstrCmdLine, int nCmdShow)
{
	HRESULT hRes = ::CoInitialize(NULL);
	ATLASSERT(SUCCEEDED(hRes));

	SetProcessDpiAwareness(PROCESS_SYSTEM_DPI_AWARE);
	AtlInitCommonControls(ICC_COOL_CLASSES | ICC_BAR_CLASSES);	// add flags to support other controls

	UtilSetPriviledge(SE_DEBUG_NAME, TRUE);

	hRes = _Module.Init(NULL, hInstance);
	ATLASSERT(SUCCEEDED(hRes));

	int nRet = Run(lpstrCmdLine, nCmdShow);

	_Module.Term();
	::CoUninitialize();

	return nRet;
}
