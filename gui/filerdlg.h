#pragma once

class CFilterDlg : public CDialogImpl<CFilterDlg>, public CDialogResize<CFilterDlg>
{
public:
	enum {
		IDD = FILTER_INIT
	};

	BEGIN_MSG_MAP(CFilterDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
		COMMAND_ID_HANDLER(IDC_FILTER_ADD, OnBtnAdd)
		COMMAND_ID_HANDLER(IDC_FILTER_REMOVE, OnBtnRemove)
		COMMAND_ID_HANDLER(IDOK, OnCloseCmd)
		COMMAND_ID_HANDLER(IDCANCEL, OnCloseCmd)
		COMMAND_ID_HANDLER(IDC_FILTER_APPLY, OnCloseCmd)
		CHAIN_MSG_MAP(CDialogResize<CFilterDlg>)
	END_MSG_MAP()

	BEGIN_DLGRESIZE_MAP(CFilterDlg)
		DLGRESIZE_CONTROL(IDC_FILTER_DEST, DLSZ_SIZE_X)
		DLGRESIZE_CONTROL(IDC_FILTER_THEN, DLSZ_MOVE_X)
		DLGRESIZE_CONTROL(IDC_FILTER_RET, DLSZ_MOVE_X)
		DLGRESIZE_CONTROL(IDC_FILTER_ADD, DLSZ_MOVE_X)
		DLGRESIZE_CONTROL(IDC_FILTER_REMOVE, DLSZ_MOVE_X)
		DLGRESIZE_CONTROL(IDC_FILTER_LIST, DLSZ_SIZE_X | DLSZ_SIZE_Y)
		DLGRESIZE_CONTROL(IDOK, DLSZ_MOVE_X|DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDCANCEL, DLSZ_MOVE_X|DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_FILTER_APPLY, DLSZ_MOVE_X|DLSZ_MOVE_Y)
	END_DLGRESIZE_MAP()


	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		DlgResize_Init(false);
		
		m_ComboBoxSrc = GetDlgItem(IDC_FILTER_SRC);
		m_ComboBoxOpt = GetDlgItem(IDC_FILTER_OPT);
		m_ComboBoxDst = GetDlgItem(IDC_FILTER_DEST);
		m_ComboBoxRet = GetDlgItem(IDC_FILTER_RET);

		m_ComboBoxSrc.AddString(TEXT("Architeture"));
		m_ComboBoxSrc.AddString(TEXT("AuthId"));
		m_ComboBoxSrc.AddString(TEXT("Category"));
		m_ComboBoxSrc.AddString(TEXT("CommandLine"));
		m_ComboBoxSrc.AddString(TEXT("Company"));
		m_ComboBoxSrc.AddString(TEXT("CompletionTime"));
		m_ComboBoxSrc.AddString(TEXT("DataTime"));
		m_ComboBoxSrc.AddString(TEXT("Description"));
		m_ComboBoxSrc.AddString(TEXT("Detail"));
		m_ComboBoxSrc.AddString(TEXT("Duration"));
		m_ComboBoxSrc.AddString(TEXT("EventClass"));
		m_ComboBoxSrc.AddString(TEXT("ImagePath"));
		m_ComboBoxSrc.AddString(TEXT("Integrity"));
		m_ComboBoxSrc.AddString(TEXT("Operation"));
		m_ComboBoxSrc.AddString(TEXT("ParentPid"));
		m_ComboBoxSrc.AddString(TEXT("Path"));
		m_ComboBoxSrc.AddString(TEXT("PID"));
		m_ComboBoxSrc.AddString(TEXT("ProcessName"));
		m_ComboBoxSrc.AddString(TEXT("RelativeTime"));
		m_ComboBoxSrc.AddString(TEXT("Result"));
		m_ComboBoxSrc.AddString(TEXT("Sequence"));
		m_ComboBoxSrc.AddString(TEXT("Session"));
		m_ComboBoxSrc.AddString(TEXT("TID"));
		m_ComboBoxSrc.AddString(TEXT("TimeOfDay"));
		m_ComboBoxSrc.AddString(TEXT("User"));
		m_ComboBoxSrc.AddString(TEXT("Version"));
		m_ComboBoxSrc.AddString(TEXT("Virtualize"));

		m_ComboBoxOpt.AddString(TEXT("Is"));
		m_ComboBoxOpt.AddString(TEXT("Is Not"));
		m_ComboBoxOpt.AddString(TEXT("Less Than"));
		m_ComboBoxOpt.AddString(TEXT("More Than"));
		m_ComboBoxOpt.AddString(TEXT("Begin With"));
		m_ComboBoxOpt.AddString(TEXT("End With"));
		m_ComboBoxOpt.AddString(TEXT("Contains"));
		m_ComboBoxOpt.AddString(TEXT("Excludes"));

		m_ComboBoxRet.AddString(TEXT("Include"));
		m_ComboBoxRet.AddString(TEXT("Exclude"));

		m_ComboBoxSrc.SetCurSel(0);
		m_ComboBoxOpt.SetCurSel(0);
		m_ComboBoxRet.SetCurSel(0);

		return 0;
	}

	LRESULT OnCloseCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		EndDialog(wID);
		return 0;
	}

	LRESULT OnBtnAdd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		return 0;
	}

	LRESULT OnBtnRemove(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		return 0;
	}

public:
	CComboBox m_ComboBoxSrc;
	CComboBox m_ComboBoxOpt;
	CComboBox m_ComboBoxDst;
	CComboBox m_ComboBoxRet;
};