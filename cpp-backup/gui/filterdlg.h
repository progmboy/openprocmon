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
		COMMAND_ID_HANDLER(IDC_FILTER_APPLY, OnApplyCmd)

		NOTIFY_HANDLER(IDC_FILTER_LIST, NM_DBLCLK, NotifyDClickHandler)
		//NOTIFY_HANDLER(IDC_FILTER_LIST, LVN_ITEMCHANGED, NotifyItemChangedHandler)

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


	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/);

	LRESULT OnApplyCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/);
	//LRESULT OnOkCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/);

	LRESULT OnCloseCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		EndDialog(wID);
		return 0;
	}

	LRESULT OnBtnAdd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/);

	LRESULT OnBtnRemove(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{

		int iItem = m_FilterListView.GetSelectedIndex();
		RemoveItemAddShowInCombox(iItem);
		return 0;
	}

	LRESULT NotifyDClickHandler(int /*idCtrl*/, LPNMHDR pnmh, BOOL& bHandled)
	{
		LPNMITEMACTIVATE pNMItemActivate = reinterpret_cast<LPNMITEMACTIVATE>(pnmh);
		if (pNMItemActivate->iItem != -1) {
			RemoveItemAddShowInCombox(pNMItemActivate->iItem);
		}

		return TRUE;
	}

private:

	int SourceTypeStringToIndex(const CString& strValue);
	int CmpTypeStringToIndex(const CString& strValue);
	int RetTypeStringToIndex(const CString& strValue);

	void RemoveItemAddShowInCombox(int nItem)
	{
		CString strTemp;
		m_FilterListView.GetItemText(nItem, 0, strTemp);
		m_ComboBoxSrc.SetCurSel(SourceTypeStringToIndex(strTemp));

		m_FilterListView.GetItemText(nItem, 1, strTemp);
		m_ComboBoxOpt.SetCurSel(CmpTypeStringToIndex(strTemp));

		CString strFilter;
		m_FilterListView.GetItemText(nItem, 2, strFilter);
		m_ComboBoxDst.AddString(strFilter);
		m_ComboBoxDst.SetCurSel(0);

		m_FilterListView.GetItemText(nItem, 3, strTemp);
		m_ComboBoxRet.SetCurSel(RetTypeStringToIndex(strTemp));

		//auto retType = static_cast<FILTER_RESULT_TYPE>(CmpTypeStringToIndex(strTemp));

		m_FilterListView.DeleteItem(nItem);
		m_ApplyBtn.EnableWindow(TRUE);

	}

public:
	CComboBox m_ComboBoxSrc;
	CComboBox m_ComboBoxOpt;
	CComboBox m_ComboBoxDst;
	CComboBox m_ComboBoxRet;
	CButton m_ApplyBtn;
	CListViewCtrl m_FilterListView;
	CImageList m_clsImageList;
	int m_IcoRet[2] = { 0 };
};