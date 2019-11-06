#pragma once

#include "dataview.h"

extern
HICON
UtilGetDefaultIcon(
	BOOL bSmall
);

class CPropProcDlg : public CDialogImpl<CPropProcDlg>, public CDialogResize<CPropProcDlg>
{
public:
	enum {
		IDD = PROP_PROCESS
	};

	BEGIN_MSG_MAP(CPropProcDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
		CHAIN_MSG_MAP(CDialogResize<CPropProcDlg>)
	END_MSG_MAP()

	BEGIN_DLGRESIZE_MAP(CPropProcDlg)
		DLGRESIZE_CONTROL(IDC_PROCESS_PATH, DLSZ_SIZE_X)
		DLGRESIZE_CONTROL(IDC_PROCESS_CMDLINE, DLSZ_SIZE_X)
		DLGRESIZE_CONTROL(IDC_PROCESS_MODULES, DLSZ_SIZE_X | DLSZ_SIZE_Y)
		DLGRESIZE_CONTROL(IDC_PROC_GROUP_BOX, DLSZ_SIZE_X)
	END_DLGRESIZE_MAP()


	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		DlgResize_Init(false);

		CRefPtr<CEventView> pView = DATAVIEW().GetSelectView();
		if (pView.IsNull()){
			return 0;
		}

		CString strTmp;
		CStatic ImgCtl = this->GetDlgItem(IDC_PROCESS_ICON);

		CBuffer& clsIconBuffer = pView->GetProcIcon(FALSE);

		HICON hIcon = NULL;
		if (!clsIconBuffer.Empty()) {

			//
			// Load from memory
			//

			int cxLarge = GetSystemMetrics(SM_CXICON);
			int cyLarge = GetSystemMetrics(SM_CYICON);

			hIcon = CreateIconFromResourceEx(clsIconBuffer.GetBuffer(),
				clsIconBuffer.GetBufferLen(), TRUE, 0x30000, cxLarge, cyLarge, 0);
		}else{
			hIcon = UtilGetDefaultIcon(FALSE);
		}

		ImgCtl.SetIcon(hIcon);

		this->GetDlgItem(IDC_PROCESS_DESC).SetWindowText( pView->GetOperationStrResult(emDescription));
		this->GetDlgItem(IDC_PROCESS_COMPANY).SetWindowText(pView->GetOperationStrResult(emCompany));
		this->GetDlgItem(IDC_PROCESS_NAME).SetWindowText(pView->GetOperationStrResult(emProcessName));
		this->GetDlgItem(IDC_PROCESS_VERSION).SetWindowText(pView->GetOperationStrResult(emVersion));

		this->GetDlgItem(IDC_PROCESS_PATH).SetWindowText(pView->GetOperationStrResult(emImagePath));
		this->GetDlgItem(IDC_PROCESS_CMDLINE).SetWindowText(pView->GetOperationStrResult(emCommandLine));


		this->GetDlgItem(IDC_PROCESS_PID).SetWindowText(pView->GetOperationStrResult(emPID));
		this->GetDlgItem(IDC_PROCESS_PPID).SetWindowText(pView->GetOperationStrResult(emParentPid));
		this->GetDlgItem(IDC_PROCESS_SESSION).SetWindowText(pView->GetOperationStrResult(emSession));

		
		//
		// User
		//
		
		this->GetDlgItem(IDC_PROCESS_USER).SetWindowText(pView->GetOperationStrResult(emUser));

		//
		// StartTime
		//
		
		this->GetDlgItem(IDC_PROCESS_STARTTIME).SetWindowText(pView->GetOperationStrResult(emDataTime));

		//
		// Ended
		//
		
		strTmp = pView->IsProcessExit() ? UtilConvertDay(pView->GetProcessExitTime()) : TEXT("Runing");
		this->GetDlgItem(IDC_PROCESS_ENDED).SetWindowText(strTmp);

		this->GetDlgItem(IDC_PROCESS_ARCH).SetWindowText(pView->GetOperationStrResult(emArchiteture));
		this->GetDlgItem(IDC_PROCESS_AUTHID).SetWindowText(pView->GetOperationStrResult(emAuthId));
		this->GetDlgItem(IDC_PROCESS_VIRTUALIZED).SetWindowText(pView->GetOperationStrResult(emVirtualize));

		
		//
		// Integrity
		//
		
		this->GetDlgItem(IDC_PROCESS_INTERGRITY).SetWindowText(pView->GetOperationStrResult(emIntegrity));

		
		//
		// set list control
		//
		
		m_ListCtrl = this->GetDlgItem(IDC_PROCESS_MODULES);

		m_ListCtrl.SetExtendedListViewStyle(LVS_EX_FULLROWSELECT);
		m_ListCtrl.InsertColumn(0, TEXT("Module"), 0, 150);
		m_ListCtrl.InsertColumn(1, TEXT("Address"), 0, 150);
		m_ListCtrl.InsertColumn(2, TEXT("Size"), 0, 100);
		m_ListCtrl.InsertColumn(3, TEXT("Path"), 0, 200);
		m_ListCtrl.InsertColumn(4, TEXT("Company"), 0, 100);
		m_ListCtrl.InsertColumn(5, TEXT("Version"), 0, 100);
		m_ListCtrl.InsertColumn(6, TEXT("Timestamp"), 0, 180);

		std::vector<CModule>& ModuleList = pView->GetModuleList();
		int nIndex = 0;
		for (auto it = ModuleList.begin(); it != ModuleList.end(); it++)
		{
			m_ListCtrl.InsertItem(nIndex, PathFindFileName(it->GetPath()));

			strTmp.Format(TEXT("0x%p"), it->GetImageBase());
			m_ListCtrl.SetItemText(nIndex, 1, strTmp);
			
			strTmp.Format(TEXT("%08x"), it->GetSize());
			m_ListCtrl.SetItemText(nIndex, 2, strTmp);

			m_ListCtrl.SetItemText(nIndex, 3, it->GetPath());

			nIndex++;
		}

		return 0;
	}

	CString CopyAll()
	{
		CString strCopy;
		CString strTemp;
		CString strItem;

		GetDlgItemText(IDC_PROCESS_DESC, strItem);
		strTemp.Format(TEXT("Description: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_COMPANY, strItem);
		strTemp.Format(TEXT("Compnay: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_NAME, strItem);
		strTemp.Format(TEXT("Process Name: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_VERSION, strItem);
		strTemp.Format(TEXT("Version: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_ARCH, strItem);
		strTemp.Format(TEXT("Arch: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_AUTHID, strItem);
		strTemp.Format(TEXT("Auth Id: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_VIRTUALIZED, strItem);
		strTemp.Format(TEXT("Virtualized: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_INTERGRITY, strItem);
		strTemp.Format(TEXT("Integrity: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_PATH, strItem);
		strTemp.Format(TEXT("Image Path: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_CMDLINE, strItem);
		strTemp.Format(TEXT("CommandLine: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_PID, strItem);
		strTemp.Format(TEXT("Pid: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_PPID, strItem);
		strTemp.Format(TEXT("Parent Pid: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_SESSION, strItem);
		strTemp.Format(TEXT("Session id: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_USER, strItem);
		strTemp.Format(TEXT("User: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_STARTTIME, strItem);
		strTemp.Format(TEXT("StartTime: %s\n"), strItem);
		strCopy += strTemp;

		GetDlgItemText(IDC_PROCESS_ENDED, strItem);
		strTemp.Format(TEXT("Ended: %s\n"), strItem);
		strCopy += strTemp;

		for (int i = 0; i < m_ListCtrl.GetItemCount(); i++)
		{
			strTemp.Format(TEXT("%d"), i);
			
			for (int j = 0; j < m_ListCtrl.GetHeader().GetItemCount(); j++)
			{
				m_ListCtrl.GetItemText(i, j, strItem);
				strTemp += TEXT(" ");
				strTemp += strItem;
			}

			strTemp += TEXT("\n");
			strCopy += strTemp;
		}

		return strCopy;
	}

private:
	CListViewCtrl m_ListCtrl;
};