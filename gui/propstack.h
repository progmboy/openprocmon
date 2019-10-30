#pragma once

#include "dataview.h"

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


	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		DlgResize_Init();

		CRefPtr<CEventView> pView = DATAVIEW().GetSelectView();

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

		int nIndex = 0;
		for (auto it = pStackFrame.begin(); it != pStackFrame.end(); it++)
		{
			CString strTmp;

			strTmp.Format(TEXT("%d"), nIndex);
			pListCtrl->InsertItem(nIndex, strTmp);

			strTmp.Format(TEXT("0x%p"), *it);
			pListCtrl->SetItemText(nIndex, 3, strTmp);

			nIndex++;
		}

		return TRUE;
	}
};