
#include "stdafx.h"
#include "resource.h"
#include <psapi.h>
#include <DbgHelp.h>
#include "propstack.h"

#pragma comment(lib, "dbghelp.lib")

BOOL CProcessInfo::LookupSymbolByAddress(
	IN LPVOID lpAddress,
	OUT CString& strSymbol
)
{
	ULONG_PTR imageLoadBase = 0;
	ULONG_PTR imageBase;
	ULONG_PTR Offset;

	//
	// get the module 
	//

	CRefPtr<CModuleInfo> pModule = LookupModuleByAddress(lpAddress);
	if (pModule.IsNull()) {
		return FALSE;
	}

	Offset = (ULONG_PTR)lpAddress - (ULONG_PTR)pModule->getBaseAddress();

	//
	// Init the symbol
	//

	strSymbol.Format(TEXT("%s+%x"), PathFindFileName(pModule->getPath()), (ULONG)Offset);

	//
	// Load the image in our process
	//

	imageLoadBase = (ULONG_PTR)LoadLibraryEx(pModule->getPath(),
		NULL,
		DONT_RESOLVE_DLL_REFERENCES);

	if (!imageLoadBase) {
		LogMessage(L_WARN, TEXT("Failed to load image \"%s\" to memory err 0x%x"),
			(LPCTSTR)pModule->getPath(), GetLastError());
		return FALSE;
	}

	MODULEINFO ModInfo;
	if (!GetModuleInformation(GetCurrentProcess(), (HMODULE)imageLoadBase,
		&ModInfo, sizeof(ModInfo))) {
		return FALSE;
	}

	//
	// Attach symbols to our module
	//

	CStringW strModuleName = CT2W(pModule->getPath());
	LPCWSTR lpModuleName = PathFindFileNameW(strModuleName);

	imageBase = SymLoadModuleExW(GetCurrentProcess(),
		NULL,
		lpModuleName,
		lpModuleName,
		imageLoadBase,
		ModInfo.SizeOfImage,//0,
		NULL,
		0);

	if (imageBase != imageLoadBase) {
		FreeLibrary((HMODULE)imageLoadBase);
		LogMessage(L_ERROR, TEXT("Failed load symbols for %s"), (LPCTSTR)pModule->getPath());
		return FALSE;
	}

	//
	// get the symbol name
	//

	ULONG_PTR Displacement = 0;
	SYMBOL_INFO_PACKAGEW SymbolInfo;

	RtlZeroMemory(&SymbolInfo, sizeof(SymbolInfo));
	SymbolInfo.si.SizeOfStruct = sizeof(SYMBOL_INFO);
	SymbolInfo.si.MaxNameLen = sizeof(SymbolInfo.name);

	if (SymFromAddrW(GetCurrentProcess(), imageLoadBase + Offset,
		&Displacement, &SymbolInfo.si)) {
		CStringW strStmbolW;

		if (Displacement) {
			strStmbolW.Format(L"%s!%s+%.8llx", lpModuleName,
				SymbolInfo.si.Name, Displacement);
		}else{
			strStmbolW.Format(L"%s!%s", lpModuleName,
				SymbolInfo.si.Name);
		}

		strSymbol = CW2T(strStmbolW);
	}

	FreeLibrary((HMODULE)imageLoadBase);
	SymUnloadModule64(GetCurrentProcess(), imageBase);

	return TRUE;
}

typedef struct _RTL_PROCESS_MODULE_INFORMATION {
	HANDLE Section;                 // Not filled in
	PVOID MappedBase;
	PVOID ImageBase;
	ULONG ImageSize;
	ULONG Flags;
	USHORT LoadOrderIndex;
	USHORT InitOrderIndex;
	USHORT LoadCount;
	USHORT OffsetToFileName;
	UCHAR  FullPathName[256];
} RTL_PROCESS_MODULE_INFORMATION, * PRTL_PROCESS_MODULE_INFORMATION;

typedef struct _RTL_PROCESS_MODULES {
	ULONG NumberOfModules;
	RTL_PROCESS_MODULE_INFORMATION Modules[1];
} RTL_PROCESS_MODULES, * PRTL_PROCESS_MODULES;
#define STATUS_INFO_LENGTH_MISMATCH ((NTSTATUS)0xC0000004L)

BOOL CProcessInfo::ListKernelModule()
{
	DWORD dwNeed = 0;
	DWORD dwBytes = 100;
	PRTL_PROCESS_MODULES lpModuleInfo = NULL;
	BOOL bOk = FALSE;
	NTSTATUS Status;

	do {

		if (lpModuleInfo) {
			LocalFree(lpModuleInfo);
		}

		lpModuleInfo = (PRTL_PROCESS_MODULES)LocalAlloc(0, dwBytes);
		if (!lpModuleInfo) {
			break;
		}

		Status = NtQuerySystemInformation((SYSTEM_INFORMATION_CLASS)11, lpModuleInfo, dwBytes, &dwNeed);
		if (!NT_SUCCESS(Status)) {
			if (Status == STATUS_INFO_LENGTH_MISMATCH) {
				dwBytes = dwNeed;
				continue;
			}
		}else{
			bOk = TRUE;
			break;
		}

	} while (TRUE);

	if (bOk) {
		for (int i = 0; i < (int)lpModuleInfo->NumberOfModules; i++) {

			CString strDosPath;
			CString strNtPath;

			strNtPath = lpModuleInfo->Modules[i].FullPathName;

			if (UtilConvertNtInternalPathToDosPath(strNtPath, strDosPath)) {
				CRefPtr<CModuleInfo> pModuleInfo = new CModuleInfo;
				if (pModuleInfo->Init(strDosPath, lpModuleInfo->Modules[i].ImageBase, lpModuleInfo->Modules[i].ImageSize)) {
					m_ModuleList.push_back(pModuleInfo);
				}
			}
		}
	}



	if (lpModuleInfo) {
		LocalFree(lpModuleInfo);
	}

	return bOk;
}

BOOL 
CProcessInfo::ListModule(
	DWORD dwProcessId
)
{
	BOOL bRet = FALSE;
	HMODULE* phModules = NULL;
	DWORD dwSize = 0x200 * sizeof(HMODULE);
	DWORD cbNeeded = 0;
	BOOL bOpened = FALSE;

	m_ModuleList.clear();

	//
	// List Kernel module first
	//
	
	ListKernelModule();

	HANDLE hProcess = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION, FALSE, dwProcessId);
	if (!hProcess) {
		return FALSE;
	}

	//m_ModuleList.clear();

	do
	{

		if (phModules) {
			LocalFree(phModules);
		}

		//
		// Reallocate buffer
		//

		phModules = (HMODULE*)LocalAlloc(0, dwSize);
		if (!phModules) {
			bRet = FALSE;
			break;
		}

		//
		// get need size of buffer
		//

		bRet = EnumProcessModulesEx(hProcess, phModules, dwSize, &cbNeeded, LIST_MODULES_ALL);
		if (!bRet) {
			break;
		}

		if (cbNeeded > dwSize) {
			dwSize += (0x200 * sizeof(HMODULE));
		}
		else {
			break;
		}

	} while (TRUE);


	if (bRet) {
		for (int i = 0; i < (cbNeeded / sizeof(HMODULE)); i++)
		{
			CRefPtr<CModuleInfo> pModule = new CModuleInfo;

			//
			// Init module information
			//

			if (pModule->Init(hProcess, phModules[i])) {

				//
				// save the module information
				//

				m_ModuleList.push_back(pModule);
			}
			else {
				LogMessage(L_WARN, TEXT("Faile to init module 0x%p err 0x%x"), phModules[i], GetLastError());
			}
		}
	}

	if (phModules) {
		LocalFree(phModules);
	}

	if (bOpened) {
		CloseHandle(hProcess);
	}

	return bRet;
}

CRefPtr<CModuleInfo>
CProcessInfo::LookupModuleByAddress(
	IN LPVOID lpAddress
)
{
	for (auto it = m_ModuleList.begin(); it != m_ModuleList.end(); it++) {
		if ((*it)->IsAddressIn(lpAddress)) {
			return *it;
		}
	}
	return NULL;
}

BOOL 
CProcessInfo::ListModuleFromLog(
	std::vector<CModule>& modList
)
{
	m_ModuleList.clear();

	//
	// List Kernel module first
	//

	ListKernelModule();

	for (auto it = modList.begin(); it != modList.end(); it++)
	{
		CRefPtr<CModuleInfo> pModule = new CModuleInfo;
		if (pModule->Init(*it)) {
			m_ModuleList.push_back(pModule);
		}
	}

	return TRUE;
}


void CResolveSymbolThread::Run()
{
	CPropStackDlg* pDlg = reinterpret_cast<CPropStackDlg*>(getParam());
	int i = 0;
	for (auto it = m_FrameStack.begin(); it != m_FrameStack.end(); it++, i++)
	{
		CString strSymbol;
		if (m_ProcInfo->LookupSymbolByAddress(*it, strSymbol)) {

			LPCTSTR lpszDup = _tcsdup(strSymbol.GetBuffer());
			pDlg->PostMessage(WM_SYMBOL_PARSE, (WPARAM)i, (LPARAM)lpszDup);
		}else{
			pDlg->PostMessage(WM_SYMBOL_PARSE, (WPARAM)i, NULL);
		}
	}
}

void CResolveSymbolThread::SetProcInf(CRefPtr<CProcessInfo> pProcInfo)
{
	m_ProcInfo = pProcInfo;
}

void CResolveSymbolThread::SetFrameStack(std::vector<PVOID>& FrameStack)
{
	m_FrameStack = FrameStack;
}

BOOL CModuleInfo::Init(CModule& Module)
{
	m_pBase = Module.GetImageBase();
	m_Size = Module.GetSize();
	m_strPath = Module.GetPath();

	return TRUE;
}

BOOL CModuleInfo::Init(IN HANDLE hProcess, IN HMODULE hModule)
{
	TCHAR szModName[MAX_PATH] = { 0 };
	MODULEINFO modInfo;

	//
	// query module basic information
	//

	if (!GetModuleInformation(hProcess, hModule, &modInfo, sizeof(modInfo))) {
		LogMessage(L_ERROR, TEXT("Failed to get module information err 0x%x"), GetLastError());
		return FALSE;
	}

	m_pBase = modInfo.lpBaseOfDll;
	m_Size = modInfo.SizeOfImage;

	//
	// query module image name
	//

	if (!GetModuleFileNameEx(hProcess, hModule, szModName, MAX_PATH)) {
		LogMessage(L_ERROR, TEXT("Failed to get module filename err 0x%x"), GetLastError());
		return FALSE;
	}

	m_strPath = szModName;

	return TRUE;
}

BOOL CModuleInfo::Init(IN const CString& strPath, IN PVOID pImageBase, IN ULONG Size)
{
	m_pBase = pImageBase;
	m_Size = Size;
	m_strPath = strPath;
	return TRUE;
}

BOOL CPropStackDlg::InitSymbol()
{
	//
	// Initialize symbol engine
	//

	BOOL b;
	DWORD Options = SymGetOptions();

	//
	// SYMOPT_DEBUG option asks DbgHelp to print additional troubleshooting 
	// messages to debug output - use the debugger's Debug Output window 
	// to view the messages 
	//

	Options |= SYMOPT_DEFERRED_LOADS;
	SymSetOptions(Options);

	b = SymInitializeW(GetCurrentProcess(), NULL, TRUE);
	if (b == FALSE) {
		LogMessage(L_ERROR, TEXT("Failed to initialize symbol engine: %lx"), GetLastError());
	}

	return b;
}

void CPropStackDlg::CleanSymbols()
{
	SymCleanup(GetCurrentProcess());
}

LRESULT CPropStackDlg::OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
{
	DlgResize_Init();

	InitSymbol();

	CRefPtr<CEventView> pView = DATAVIEW().GetSelectView();
	m_ListCtrl = this->GetDlgItem(IDC_PROP_STACKLIST);
	m_StatusCtl = this->GetDlgItem(IDC_STATIC_STAUS);
	
	m_StatusCtl.SetWindowText(TEXT(""));

	m_ListCtrl.SetExtendedListViewStyle(LVS_EX_FULLROWSELECT);
	m_ListCtrl.InsertColumn(0, TEXT("Frame"), 0, 50);
	m_ListCtrl.InsertColumn(1, TEXT("Module"), 0, 100);
	m_ListCtrl.InsertColumn(2, TEXT("Location"), 0, 200);
	m_ListCtrl.InsertColumn(3, TEXT("Address"), 0, 150);
	m_ListCtrl.InsertColumn(4, TEXT("Path"), 0, 400);

	std::vector<PVOID> pStackFrame;
	pView->GetCallStack(pStackFrame);

	m_ProcInfo = new CProcessInfo;

	//
	// 首先判断进程是否在监控前已存在.
	// 如果是的话,再判断进程是否退出.
	// 如果没有退出的话,这里我们通过打开进程枚举模块的形式获取模块信息
	//

	if (pView->IsProcessFromInit() && !pView->IsProcessExit()) {

		//
		// 从进程中枚举模块
		//

		m_ProcInfo->ListModule(pView->GetProcessId());

	}else{

		//
		// 直接从记录中查找
		//

		m_ProcInfo->ListModuleFromLog(pView->GetModuleList());

	}

	int nIndex = 0;
	for (auto it = pStackFrame.begin(); it != pStackFrame.end(); it++)
	{
		CString strTmp;

		strTmp.Format(TEXT("%d"), nIndex);
		m_ListCtrl.InsertItem(nIndex, strTmp);

		strTmp.Format(TEXT("0x%p"), *it);
		m_ListCtrl.SetItemText(nIndex, 3, strTmp);

		CRefPtr<CModuleInfo> pModuleInfo = m_ProcInfo->LookupModuleByAddress(*it);
		if (!pModuleInfo.IsNull()) {
			m_ListCtrl.SetItemText(nIndex, 1, PathFindFileName(pModuleInfo->getPath()));
			m_ListCtrl.SetItemText(nIndex, 4, pModuleInfo->getPath());

			ULONG_PTR pOffset = (ULONG_PTR)(*it) - (ULONG_PTR)pModuleInfo->getBaseAddress();
			strTmp.Format(TEXT("%s+0x%p"), PathFindFileName(pModuleInfo->getPath()), pOffset);
			m_ListCtrl.SetItemText(nIndex, 2, strTmp);
		}else{
			m_ListCtrl.SetItemText(nIndex, 1, TEXT("<Unknown>"));
		}

		nIndex++;
	}

	m_ResoveSymbolThread.setParam((PVOID)this);
	m_ResoveSymbolThread.SetProcInf(m_ProcInfo);
	m_ResoveSymbolThread.SetFrameStack(pStackFrame);
	m_ResoveSymbolThread.SetTimeout(3);
	m_ResoveSymbolThread.Start();

	return TRUE;
}

LRESULT CPropStackDlg::OnDestroy(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
{
	m_ResoveSymbolThread.Stop();
	CleanSymbols();
	return TRUE;
}

CString CPropStackDlg::CopyAll()
{
	CString strCopy;
	
	for (int i = 0; i < m_ListCtrl.GetItemCount(); i++)
	{
		CString strTemp;
		for (int j = 0; j < m_ListCtrl.GetHeader().GetItemCount(); j++)
		{
			CString strItem;
			m_ListCtrl.GetItemText(i, j, strItem);
			strTemp += TEXT(" ");
			strTemp += strItem;
		}

		strTemp += TEXT("\n");
		strCopy += strTemp;
	}

	return strCopy;
}

LRESULT CPropStackDlg::OnSymbolParse(UINT /*uMsg*/, WPARAM wParam, LPARAM lParam, BOOL& /*bHandled*/)
{
	int nIndex = (int)wParam;
	LPCTSTR lpszSymbol = (LPCTSTR)lParam;
	if (lpszSymbol){
		m_ListCtrl.SetItemText(nIndex, 2, lpszSymbol);
		free((PVOID)lpszSymbol);
	}

	if (nIndex < m_ListCtrl.GetItemCount()-1){
		CString strAddress;
		m_ListCtrl.GetItemText(nIndex, 3, strAddress);

		CString strShow;

		strShow.Format(TEXT("Parsing symbol for %s"), strAddress.GetBuffer());
		m_StatusCtl.SetWindowText(strShow);
	}else{
		m_StatusCtl.SetWindowText(TEXT(""));
	}

	return TRUE;

}