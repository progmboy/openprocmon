#pragma once

#include "dataview.h"
#include <psapi.h>

#if 0
class CModuleInfo : public CRefBase
{
public:
	CModuleInfo() {};
	virtual ~CModuleInfo() {};

	BOOL Init(IN HANDLE hProcess, IN HMODULE hModule)
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

		//
		// Save module info
		//

		m_hModule = hModule;

		return TRUE;
	}

	CString getName()
	{
		return PathFindFileName(m_strPath);
	}

	const CPath& getPath()
	{
		return m_strPath;
	}
	LPVOID getBaseAddress()
	{
		return m_pBase;
	}

	ULONG getSize()
	{
		return m_Size;
	}

	BOOL IsAddressIn(LPVOID lpAddress)
	{
		if ((ULONG_PTR)lpAddress >= (ULONG_PTR)m_pBase &&
			(ULONG_PTR)(lpAddress) < ((ULONG_PTR)m_pBase + m_Size)) {
			return TRUE;
		}
		return FALSE;
	}

private:

	/** specific the module image path*/
	CPath m_strPath;

	/** specific the module base address*/
	LPVOID m_pBase = NULL;

	/** specific the module image size*/
	ULONG m_Size = 0;

	/** specific the module handle same as m_pBase*/
	HMODULE m_hModule = NULL;
};
#endif


class CPropStackDlg : public CDialogImpl<CPropStackDlg>, public CDialogResize<CPropStackDlg>
{
public:
	enum {
		IDD = PROP_STACKTRACE
	};

	BEGIN_MSG_MAP(CPropStackDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
		CHAIN_MSG_MAP(CDialogResize<CPropStackDlg>)
	END_MSG_MAP()

	BEGIN_DLGRESIZE_MAP(CPropStackDlg)
		DLGRESIZE_CONTROL(IDC_PROP_STACKLIST, DLSZ_SIZE_X | DLSZ_SIZE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_PROPS, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_SAVE, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_SEARCH, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_BTN_SOURCE, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_STATIC_STAUS, DLSZ_MOVE_Y | DLSZ_SIZE_X)
	END_DLGRESIZE_MAP()

#if 0
	BOOL ListModule(DWORD dwProcessId)
	{
		BOOL bRet = FALSE;
		HMODULE* phModules = NULL;
		DWORD dwSize = 0x200 * sizeof(HMODULE);
		DWORD cbNeeded = 0;
		BOOL bOpened = FALSE;

		HANDLE hProcess = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, dwProcessId);
		if (!hProcess) {
			return FALSE;
		}

		m_ModuleList.clear();

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

			bRet = EnumProcessModules(hProcess, phModules, dwSize, &cbNeeded);
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
				}else{
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
	lookupModuleByAddress(
			IN LPVOID lpAddress
		)
	{
		for (auto it = m_ModuleList.begin(); it != m_ModuleList.end(); it++) {
			if ((*it)->IsAddressIn(lpAddress)) {
				return (*it);
			}
		}
		return NULL;
	}
#endif

	BOOL
	lookupModuleByAddress(
		IN LPVOID lpAddress,
		IN CModule& cModule
	)
	{
		for (auto it = m_ModuleList.begin(); it != m_ModuleList.end(); it++) {
// 			if ((*it)->IsAddressIn(lpAddress)) {
// 				return (*it);
// 			}

			ULONG_PTR pBase = (ULONG_PTR)((*it).GetImageBase());

			if ((ULONG_PTR)lpAddress >= pBase &&
				(ULONG_PTR)(lpAddress) < (pBase + (*it).GetSize())) {
				cModule = (*it);
				return TRUE;
			}
		}
		return FALSE;
	}


	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		DlgResize_Init();

		BOOL bExit = FALSE;
		CRefPtr<CEventView> pView = DATAVIEW().GetSelectView();

		//bExit = ListModule(pView->GetProcessId());

		CWindow wnd1 = this->GetDlgItem(IDC_PROP_STACKLIST);
		CListViewCtrl* pListCtrl = (CListViewCtrl*)&wnd1;

		pListCtrl->SetExtendedListViewStyle(LVS_EX_FULLROWSELECT);
		pListCtrl->InsertColumn(0, TEXT("Frame"), 0, 50);
		pListCtrl->InsertColumn(1, TEXT("Module"), 0, 100);
		pListCtrl->InsertColumn(2, TEXT("Location"), 0, 200);
		pListCtrl->InsertColumn(3, TEXT("Address"), 0, 150);
		pListCtrl->InsertColumn(4, TEXT("Path"), 0, 400);

		std::vector<PVOID> pStackFrame;
		pView->GetCallStack(pStackFrame);
		m_ModuleList = pView->GetModuleList();

		//
		// 首先判断进程是否存在.
		//
		
		int nIndex = 0;
		for (auto it = pStackFrame.begin(); it != pStackFrame.end(); it++)
		{
			CString strTmp;

			strTmp.Format(TEXT("%d"), nIndex);
			pListCtrl->InsertItem(nIndex, strTmp);

			strTmp.Format(TEXT("0x%p"), *it);
			pListCtrl->SetItemText(nIndex, 3, strTmp);

			CModule cModule;
			if (lookupModuleByAddress(*it, cModule)){
				pListCtrl->SetItemText(nIndex, 1, PathFindFileName(cModule.GetPath()));
				pListCtrl->SetItemText(nIndex, 4, cModule.GetPath());
			}

			nIndex++;
		}

		return TRUE;
	}

private:
	//std::vector<CRefPtr<CModuleInfo>> m_ModuleList;
	std::vector<CModule> m_ModuleList;
};