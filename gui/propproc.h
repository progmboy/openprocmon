#pragma once

#include "status.h"
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
		CWindow wnd = this->GetDlgItem(IDC_PROCESS_ICON);
		CStatic* pImg = (CStatic*)&wnd;

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

		pImg->SetIcon(hIcon);

		this->GetDlgItem(IDC_PROCESS_DESC).SetWindowText(MapMonitorResult(emDescription, pView));
		this->GetDlgItem(IDC_PROCESS_COMPANY).SetWindowText(MapMonitorResult(emCompany, pView));
		this->GetDlgItem(IDC_PROCESS_NAME).SetWindowText(MapMonitorResult(emProcessName, pView));
		this->GetDlgItem(IDC_PROCESS_VERSION).SetWindowText(MapMonitorResult(emVersion, pView));

		this->GetDlgItem(IDC_PROCESS_PATH).SetWindowText(MapMonitorResult(emImagePath, pView));
		this->GetDlgItem(IDC_PROCESS_CMDLINE).SetWindowText(MapMonitorResult(emCommandLine, pView));


		this->GetDlgItem(IDC_PROCESS_PID).SetWindowText(MapMonitorResult(emPID, pView));
		this->GetDlgItem(IDC_PROCESS_PPID).SetWindowText(MapMonitorResult(emParentPid, pView));
		this->GetDlgItem(IDC_PROCESS_SESSION).SetWindowText(MapMonitorResult(emSession, pView));

		
		//
		// User
		//
		
		this->GetDlgItem(IDC_PROCESS_USER).SetWindowText(MapMonitorResult(emUser, pView));

		//
		// StartTime
		//
		
		this->GetDlgItem(IDC_PROCESS_STARTTIME).SetWindowText(MapMonitorResult(emDataTime, pView));

		//
		// Ended
		//
		
		strTmp = pView->IsProcessExit() ? TEXT("Runing") : UtilConvertTimeOfDay(pView->GetProcessExitTime());
		this->GetDlgItem(IDC_PROCESS_ENDED).SetWindowText(strTmp);

		this->GetDlgItem(IDC_PROCESS_ARCH).SetWindowText(MapMonitorResult(emArchiteture, pView));
		this->GetDlgItem(IDC_PROCESS_AUTHID).SetWindowText(MapMonitorResult(emAuthId, pView));
		this->GetDlgItem(IDC_PROCESS_VIRTUALIZED).SetWindowText(MapMonitorResult(emVirtualize, pView));

		
		//
		// Integrity
		//
		
		this->GetDlgItem(IDC_PROCESS_INTERGRITY).SetWindowText(MapMonitorResult(emIntegrity, pView));

		
		//
		// set list control
		//
		
		CWindow wnd1 = this->GetDlgItem(IDC_PROCESS_MODULES);
		CListViewCtrl* pListCtrl = (CListViewCtrl*)&wnd1;

		pListCtrl->SetExtendedListViewStyle(LVS_EX_FULLROWSELECT);
		pListCtrl->InsertColumn(0, TEXT("Module"), 0, 150);
		pListCtrl->InsertColumn(1, TEXT("Address"), 0, 150);
		pListCtrl->InsertColumn(2, TEXT("Size"), 0, 100);
		pListCtrl->InsertColumn(3, TEXT("Path"), 0, 200);
		pListCtrl->InsertColumn(4, TEXT("Company"), 0, 100);
		pListCtrl->InsertColumn(5, TEXT("Version"), 0, 100);
		pListCtrl->InsertColumn(6, TEXT("Timestamp"), 0, 180);

		std::vector<CModule>& ModuleList = pView->GetModuleList();
		int nIndex = 0;
		for (auto it = ModuleList.begin(); it != ModuleList.end(); it++)
		{
			pListCtrl->InsertItem(nIndex, PathFindFileName(it->GetPath()));

			strTmp.Format(TEXT("0x%p"), it->GetImageBase());
			pListCtrl->SetItemText(nIndex, 1, strTmp);
			
			strTmp.Format(TEXT("%08x"), it->GetSize());
			pListCtrl->SetItemText(nIndex, 2, strTmp);

			pListCtrl->SetItemText(nIndex, 3, it->GetPath());

			nIndex++;
		}

		return 0;
	}
};