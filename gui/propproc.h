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


		CWindow wnd = this->GetDlgItem(1029);
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

		this->GetDlgItem(1035).SetWindowText(pView->GetDisplayName());
		this->GetDlgItem(1034).SetWindowText(pView->GetCompanyName());
		this->GetDlgItem(IDC_PROCESS_NAME).SetWindowText(pView->GetProcessName());
		this->GetDlgItem(IDC_PROCESS_VERSION).SetWindowText(pView->GetVersion());

		this->GetDlgItem(IDC_PROCESS_PATH).SetWindowText(pView->GetImagePath());
		this->GetDlgItem(IDC_PROCESS_CMDLINE).SetWindowText(pView->GetCommandLine());

		CString strTmp;

		strTmp.Format(TEXT("%d"), pView->GetProcessId());
		this->GetDlgItem(1137).SetWindowText(strTmp);

		strTmp.Format(TEXT("%d"), pView->GetParentProcessId());
		this->GetDlgItem(1136).SetWindowText(strTmp);

		strTmp.Format(TEXT("%d"), pView->GetSessionId());
		this->GetDlgItem(1135).SetWindowText(strTmp);

		strTmp = TEXT("TODO");
		this->GetDlgItem(1134).SetWindowText(strTmp);

		strTmp = TEXT("TODO");
		this->GetDlgItem(1133).SetWindowText(strTmp);

		strTmp = TEXT("TODO");
		this->GetDlgItem(1142).SetWindowText(strTmp);

		strTmp = pView->IsWow64() ? TEXT("32-bit") : TEXT("64-bit");
		this->GetDlgItem(1138).SetWindowText(strTmp);

		strTmp = pView->IsVirtualize() ? TEXT("True") : TEXT("False");
		this->GetDlgItem(1140).SetWindowText(strTmp);

		strTmp = TEXT("TODO");
		this->GetDlgItem(1141).SetWindowText(strTmp);

		
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

			nIndex++;
		}

		return 0;
	}
};