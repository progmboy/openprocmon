#pragma once

#include "propevent.h"
#include "propproc.h"
#include "propstack.h"

class CPropertiesDlg : public CDialogImpl<CPropertiesDlg>, public CDialogResize<CPropertiesDlg>
{
public:
	enum {
		IDD = IDD_DIALOG_PROPERTIES
	};

	BEGIN_MSG_MAP(CPropertiesDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
		NOTIFY_HANDLER(IDC_TAB_PROPERTIES, TCN_SELCHANGE, OnSelTabChange)
		COMMAND_HANDLER(IDC_PROPERITES_COPYALL, BN_CLICKED, OnCopyAllClick)
		COMMAND_HANDLER(ID_PROPERITIES_CLOSE, BN_CLICKED, OnCloseCmd)
		COMMAND_ID_HANDLER(IDCANCEL, OnCloseCmd)
		MESSAGE_HANDLER(WM_SIZE, OnSize)
		CHAIN_MSG_MAP(CDialogResize<CPropertiesDlg>)
	END_MSG_MAP()

	BEGIN_DLGRESIZE_MAP(CPropertiesDlg)
		DLGRESIZE_CONTROL(ID_PROPERITIES_CLOSE, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_PROPERITES_COPYALL, DLSZ_MOVE_X | DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_TAB_PROPERTIES, DLSZ_SIZE_X | DLSZ_SIZE_Y)
		DLGRESIZE_CONTROL(IDC_PROPERITES_PREV, DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_PROPERTIES_NEXT, DLSZ_MOVE_Y)
		DLGRESIZE_CONTROL(IDC_PROPERTIES_CHECK, DLSZ_MOVE_Y)
	END_DLGRESIZE_MAP()

	void Resize()
	{
		CRect rcItem;
		m_TabCtrl.GetItemRect(0, &rcItem);

		CRect rc;
		m_TabCtrl.GetClientRect(&rc);

		rc.top += rcItem.Height();

		for (int i = 0; i < _countof(m_DiaLogArray); i++)
		{
			m_DiaLogArray[i]->MoveWindow(rc);
		}
	}

	void SetCurTab(int index)
	{
		if (index <0 || index >= 3) {
			return;
		}

		for (int i = 0; i < _countof(m_DiaLogArray); i++)
		{
			if (i == index){
				m_DiaLogArray[i]->ShowWindow(SW_SHOW);
				m_DiaLogArray[i]->EnableWindow(TRUE);
				m_preCurSel = i;
			}else{
				m_DiaLogArray[i]->ShowWindow(SW_HIDE);
			}
		}
	}

	void PreSetCurTab(int index)
	{
		m_preCurSel = index;
	}

	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		DlgResize_Init();

		m_TabCtrl = GetDlgItem(IDC_TAB_PROPERTIES);
		m_TabCtrl.ModifyStyleEx(0, WS_EX_CONTROLPARENT);

		m_TabCtrl.AddItem(TEXT("Event"));
		m_TabCtrl.AddItem(TEXT("Process"));
		m_TabCtrl.AddItem(TEXT("Stack"));

		m_EventDlg.Create(m_TabCtrl);
		m_ProcDlg.Create(m_TabCtrl);
		m_StackDlg.Create(m_TabCtrl);

		m_DiaLogArray[0] = &m_EventDlg;
		m_DiaLogArray[1] = &m_ProcDlg;
		m_DiaLogArray[2] = &m_StackDlg;

		SetCurTab(m_preCurSel);
		m_TabCtrl.SetCurSel(m_preCurSel);

		Resize();

		CenterWindow(GetParent());

		return TRUE;
	}

	LRESULT OnSelTabChange(int /*idCtrl*/, LPNMHDR pnmh, BOOL& bHandled)
	{
		int nIndex = m_TabCtrl.GetCurSel();
		SetCurTab(nIndex);
		return 0;

	}

	LRESULT OnCloseCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		EndDialog(wID);
		return 0;
	}

	VOID CopyToClipboard(CString& strData) {

		if (strData.IsEmpty()){
			return;
		}

		OpenClipboard();
		EmptyClipboard();
		HGLOBAL hMem = GlobalAlloc(GMEM_MOVEABLE, (strData.GetLength() + 1) * sizeof(TCHAR));
		if (!hMem) {
			CloseClipboard();
			return;
		}
		
		//
		// Copy data
		//
		
		CopyMemory(GlobalLock(hMem), strData.GetBuffer(), (strData.GetLength() + 1) * sizeof(TCHAR));
		GlobalUnlock(hMem);
		
		//
		// set data
		//
		
#ifdef _UNICODE
		SetClipboardData(CF_UNICODETEXT, hMem);
#else
		SetClipboardData(CF_TEXT, hMem);
#endif
		
		//
		// Cleanup
		//
		
		CloseClipboard();
		GlobalFree(hMem);
	}

	LRESULT OnCopyAllClick(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		int nIndex = m_TabCtrl.GetCurSel();
		CString strCopy;
		switch (nIndex)
		{
		case 0:
			strCopy = m_EventDlg.CopyAll();
			break;
		case 1:
			strCopy = m_ProcDlg.CopyAll();
			break;
		case 2:
			strCopy = m_StackDlg.CopyAll();
		default:
			break;
		}

		CopyToClipboard(strCopy);

		return 0;
	}

	LRESULT OnSize(UINT uMsg, WPARAM wParam, LPARAM lParam, BOOL& bHandled)
	{

		CDialogResize<CPropertiesDlg>::OnSize(uMsg, wParam, lParam, bHandled);

		Resize();

		bHandled = TRUE;

		return 0;
	}

private:
	CTabCtrl m_TabCtrl;
 	CPropEventDlg m_EventDlg;
 	CPropProcDlg m_ProcDlg;
 	CPropStackDlg m_StackDlg;
	CWindow* m_DiaLogArray[3];
	int m_preCurSel = 0;
};

