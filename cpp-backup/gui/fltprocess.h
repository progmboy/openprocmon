#pragma once

#define WM_PROGRESSUPDATE WM_USER+1000

class CFltProcessDlg : public CDialogImpl<CFltProcessDlg>
{
public:
	enum { IDD = IDD_PROGRESS };

	BEGIN_MSG_MAP(CFltProcessDlg)
		MESSAGE_HANDLER(WM_INITDIALOG, OnInitDialog)
		MESSAGE_HANDLER(WM_PROGRESSUPDATE, OnProgressUpdate)
		MESSAGE_HANDLER(WM_DESTROY, OnDestory)

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
		if (!Total){
			return;
		}
		
		//
		// Show progressing
		//
		
		int nPos = (int)(((float)(Current+1) / (float)Total) * 100.0);
		if (pDlg->m_CurProcess != nPos){
			pDlg->m_CurProcess = nPos;
			pDlg->PostMessage(WM_PROGRESSUPDATE, (WPARAM)nPos, 0);
		}
	}

	static DWORD ThreadUpdate(LPVOID lParam)
	{
		CFltProcessDlg* pDlg = reinterpret_cast<CFltProcessDlg*>(lParam);
		DATAVIEW().ApplyNewFilter(OnFltProcessing, lParam);
		pDlg->PostMessage(WM_PROGRESSUPDATE, (WPARAM)100, 0);
		return 0;
	}

	LRESULT OnDestory(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		CloseHandle(m_hThread);
		return 0;
	}

	LRESULT OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		CenterWindow(GetParent());
		m_ProgressBar = GetDlgItem(1093);
		m_ProgressBar.SetRange(0, 100);
		m_CurProcess = 0;
		m_hThread = CreateThread(NULL, 0, ThreadUpdate, this, 0, NULL);

		return TRUE;
	}

	LRESULT OnProgressUpdate(UINT /*uMsg*/, WPARAM wParam, LPARAM lParam, BOOL& /*bHandled*/)
	{
		int nPos = (int)wParam;
		if (nPos >= 100){
			EndDialog(0);
		}else{
			m_ProgressBar.SetPos(nPos);
		}
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
	HANDLE m_hThread = NULL;
};