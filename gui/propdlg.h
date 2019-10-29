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
		COMMAND_HANDLER(ID_PROPERITIES_CLOSE, BN_CLICKED, OnCloseCmd)
		COMMAND_ID_HANDLER(IDCANCEL, OnCloseCmd)
		MESSAGE_HANDLER(WM_WINDOWPOSCHANGED, OnWindowPosChanged)
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

	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		DlgResize_Init();
		m_TabCtrl = GetDlgItem(IDC_TAB_PROPERTIES);

		m_TabCtrl.AddItem(TEXT("Event"));
		m_TabCtrl.AddItem(TEXT("Process"));
		m_TabCtrl.AddItem(TEXT("Stack"));


 		m_EventDlg.Create(m_TabCtrl);
		m_ProcDlg.Create(m_TabCtrl);
		m_StackDlg.Create(m_TabCtrl);

		CRect rcView;
		m_EventDlg.GetWindowRect(&rcView);

		m_EventDlg.ShowWindow(SW_SHOW);
		m_ProcDlg.ShowWindow(SW_HIDE);
		m_StackDlg.ShowWindow(SW_HIDE);

		CenterWindow(GetParent());

		return TRUE;
	}

	LRESULT OnSelTabChange(int /*idCtrl*/, LPNMHDR pnmh, BOOL& bHandled)
	{
		int nIndex = m_TabCtrl.GetCurSel();

		if (nIndex == 0){
			m_EventDlg.ShowWindow(SW_SHOW);
			m_ProcDlg.ShowWindow(SW_HIDE);
			m_StackDlg.ShowWindow(SW_HIDE);
		}else if (nIndex == 1){
			m_EventDlg.ShowWindow(SW_HIDE);
			m_ProcDlg.ShowWindow(SW_SHOW);
			m_StackDlg.ShowWindow(SW_HIDE);
		}else if (nIndex == 2) {
			m_EventDlg.ShowWindow(SW_HIDE);
			m_ProcDlg.ShowWindow(SW_HIDE);
			m_StackDlg.ShowWindow(SW_SHOW);
		}

		return 0;

	}

	LRESULT OnCloseCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		EndDialog(wID);
		return 0;
	}

	LRESULT OnWindowPosChanged(UINT, WPARAM, LPARAM, BOOL& bHandled)
	{
		CRect rcItem;
		m_TabCtrl.GetItemRect(0, &rcItem);
		
		CRect rc;
		m_TabCtrl.GetClientRect(&rc);

		rc.top += rcItem.Height();

		m_EventDlg.MoveWindow(&rc);
		m_ProcDlg.MoveWindow(&rc);
		m_StackDlg.MoveWindow(&rc);

		bHandled = FALSE;

		return 0;
	}

private:
	CTabCtrl m_TabCtrl;
	CPropEventDlg m_EventDlg;
	CPropProcDlg m_ProcDlg;
	CPropStackDlg m_StackDlg;
};

