#pragma once

class CPropEventDlg : public CDialogImpl<CPropEventDlg>
{
public:
	enum {
		IDD = PROP_EVENT
	};

	BEGIN_MSG_MAP(CPropEventDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
	END_MSG_MAP()

	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/);
	CString CopyAll();
};
