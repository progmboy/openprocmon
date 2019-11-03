#pragma once

class CFltProcessDlg : public CDialogImpl<CFltProcessDlg>
{
public:
	enum { IDD = IDD_PROGRESS };

	BEGIN_MSG_MAP(CFltProcessDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
		COMMAND_ID_HANDLER(IDOK, OnCloseCmd)
		COMMAND_ID_HANDLER(IDCANCEL, OnCloseCmd)
	END_MSG_MAP()

	// Handler prototypes (uncomment arguments if needed):
	//	LRESULT MessageHandler(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	//	LRESULT CommandHandler(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	//	LRESULT NotifyHandler(int /*idCtrl*/, LPNMHDR /*pnmh*/, BOOL& /*bHandled*/)

	static VOID OnFltProcessing(size_t Total, size_t Current, PVOID pParamter)
	{
		CFltProcessDlg* pDlg = reinterpret_cast<CFltProcessDlg*>(pParamter);
		if (Current >= Total){
			pDlg->EndDialog(0);
		}
		
		//
		// Show progressing
		//
		
		int nPos = (int)(((float)(Total - Current) / (float)Total) * 100.0);
		if (pDlg->m_CurProcess != nPos){
			pDlg->m_CurProcess = nPos;
			pDlg->m_ProgressBar.SetPos(nPos);
		}
	}

	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		CenterWindow(GetParent());
		m_ProgressBar = GetDlgItem(1093);
		m_ProgressBar.SetRange(0, 100);
		m_CurProcess = 0;

		DATAVIEW().ApplyNewFilter(OnFltProcessing, this);

		return TRUE;
	}

	LRESULT OnCloseCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		EndDialog(wID);
		return 0;
	}

public:
	CProgressBarCtrl m_ProgressBar;
	int m_CurProcess = 0;
};