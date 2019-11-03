// MainFrm.h : interface of the CMainFrame class
//
/////////////////////////////////////////////////////////////////////////////

#pragma once

#include <vector>
#include <atltypes.h>
#include "status.h"
#include "filtermgr.h"
#include "dataview.h"

#ifdef _DEBUG
#pragma comment(lib, "../x64/Debug/procmonsdk.lib")
#else
#pragma comment(lib, "../x64/Release/procmonsdk.lib")
#endif

#define WM_NEW_OPERATOR (WM_USER+1)

#define ID_MEMU_PROPERTIES	(WM_USER+100)
#define ID_MEMU_STACK		(WM_USER+101)
#define ID_MEMU_BOOKMARK	(WM_USER+102)
#define ID_MEMU_JUMPTO		(WM_USER+103)

#define ID_MEMU_INCLUDE		(WM_USER+105)
#define ID_MEMU_HIGHLIGHT	(WM_USER+105)
#define ID_MEMU_EXCLUDE		(WM_USER+105)

typedef struct _ICONS
{
	HICON hSmall;
	HICON hLarge;
}ICONS, *PICONS;


HICON
UtilGetDefaultIcon(
	BOOL bSmall
)
{
	static ICONS hDefault = { 0 };
	if (!hDefault.hSmall || !hDefault.hLarge) {

		SHFILEINFO psfi = { 0 };

		//
		// small
		//

		DWORD_PTR dwRet = SHGetFileInfo(TEXT(".exe"), FILE_ATTRIBUTE_NORMAL,
			&psfi, sizeof(psfi),
			SHGFI_USEFILEATTRIBUTES | SHGFI_ICON | SHGFI_SMALLICON);

		hDefault.hSmall = psfi.hIcon;

		ZeroMemory(&psfi, sizeof(psfi));

		SHGetFileInfo(TEXT(".exe"), FILE_ATTRIBUTE_NORMAL,
			&psfi, sizeof(psfi),
			SHGFI_USEFILEATTRIBUTES | SHGFI_ICON | SHGFI_LARGEICON);

		hDefault.hLarge = psfi.hIcon;
	}

	if (bSmall){
		return hDefault.hSmall;
	}else{
		return hDefault.hLarge;
	}
}


class CMainFrame : 
	public CFrameWindowImpl<CMainFrame>, 
	public CUpdateUI<CMainFrame>,
	public CMessageFilter, public CIdleHandler,
	public IEventCallback
{
public:
	DECLARE_FRAME_WND_CLASS(NULL, IDR_MAINFRAME)

	CView m_view;
	CCommandBarCtrl m_CmdBar;

	virtual BOOL PreTranslateMessage(MSG* pMsg)
	{
		if(CFrameWindowImpl<CMainFrame>::PreTranslateMessage(pMsg))
			return TRUE;

		return m_view.PreTranslateMessage(pMsg);
	}

	virtual BOOL OnIdle()
	{
		m_view.SetItemCountEx((int)DATAVIEW().GetShowViewCounts(), LVSICF_NOINVALIDATEALL | LVSICF_NOSCROLL);
		if (m_bScrollDown){
			m_view.SendMessage(WM_VSCROLL, SB_BOTTOM, NULL);
		}

		//m_view.SendMessage(WM_VSCROLL, SB_BOTTOM, NULL);
		//m_view.EnsureVisible()
		UIUpdateToolBar();
		return FALSE;
	}

	BEGIN_UPDATE_UI_MAP(CMainFrame)
		UPDATE_ELEMENT(ID_VIEW_TOOLBAR, UPDUI_MENUPOPUP)
		UPDATE_ELEMENT(ID_VIEW_STATUS_BAR, UPDUI_MENUPOPUP)
	END_UPDATE_UI_MAP()

	BEGIN_MSG_MAP(CMainFrame)
		MESSAGE_HANDLER(WM_CREATE, OnCreate)
		MESSAGE_HANDLER(WM_DESTROY, OnDestroy)
		MESSAGE_HANDLER(WM_NEW_OPERATOR, OnNewOperator)
		COMMAND_ID_HANDLER(ID_APP_EXIT, OnFileExit)
		COMMAND_ID_HANDLER(ID_FILE_NEW, OnFileNew)
		COMMAND_ID_HANDLER(ID_VIEW_TOOLBAR, OnViewToolBar)
		COMMAND_ID_HANDLER(ID_VIEW_STATUS_BAR, OnViewStatusBar)
		COMMAND_ID_HANDLER(ID_APP_ABOUT, OnAppAbout)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_SAVE, OnFileSave)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_ERASE, OnEraseShow)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_START, OnMonitorStart)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_STOP, OnMonitorStop)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_OPENEDF, OnFileOpen)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_SCROLLDOWN, OnScrollDownClick)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_SCROLLUP, OnScrollUpClick)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_FILTER, OnFilterClick)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_PROCESS, OnFilterProcessClick)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_FILE, OnFilterFileClick)
		COMMAND_ID_HANDLER(ID_BUTTON_ICONS8_REGISTRY, OnFilterRegClick)

		COMMAND_ID_HANDLER(ID_MEMU_PROPERTIES, OnEventProperties)
		NOTIFY_HANDLER(IDC_LISTCTRL, NM_RCLICK, NotifyRClickHandler)
		NOTIFY_HANDLER(IDC_LISTCTRL, LVN_GETDISPINFO, NotifyVDisplayHandler)
		NOTIFY_HANDLER(IDC_LISTCTRL, LVN_ITEMCHANGED, NotifyItemChangedHandler)
		CHAIN_MSG_MAP(CUpdateUI<CMainFrame>)
		CHAIN_MSG_MAP(CFrameWindowImpl<CMainFrame>)
	END_MSG_MAP()

// Handler prototypes (uncomment arguments if needed):
//	LRESULT MessageHandler(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
//	LRESULT CommandHandler(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
//	LRESULT NotifyHandler(int /*idCtrl*/, LPNMHDR /*pnmh*/, BOOL& /*bHandled*/)

	LRESULT OnFileSave(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		MessageBox(TEXT("TODO"));
		return 0;
	}

	LRESULT OnFilterProcessClick(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		BOOL bShow = m_wndToolBar.IsButtonChecked(ID_BUTTON_ICONS8_PROCESS);

		if (bShow){
			FILETERMGR().RemovFilter(emEventClass, emCMPIs, emRETExclude, GetClassStringMap(MONITOR_TYPE_PROCESS));
		}else{
			FILETERMGR().AddFilter(emEventClass, emCMPIs, emRETExclude, GetClassStringMap(MONITOR_TYPE_PROCESS));
		}

		CFltProcessDlg Dlg;
		Dlg.DoModal();

		m_view.SetItemCountEx((int)DATAVIEW().GetShowViewCounts(), 0);

		return 0;
	}

	LRESULT OnFilterFileClick(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		BOOL bShow = m_wndToolBar.IsButtonChecked(ID_BUTTON_ICONS8_FILE);

		if (bShow) {
			FILETERMGR().RemovFilter(emEventClass, emCMPIs, emRETExclude, GetClassStringMap(MONITOR_TYPE_FILE));
		}else {
			FILETERMGR().AddFilter(emEventClass, emCMPIs, emRETExclude, GetClassStringMap(MONITOR_TYPE_FILE));
		}

		CFltProcessDlg Dlg;
		Dlg.DoModal();

		m_view.SetItemCountEx((int)DATAVIEW().GetShowViewCounts(), 0);

		return 0;
	}

	LRESULT OnFilterRegClick(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		BOOL bShow = m_wndToolBar.IsButtonChecked(ID_BUTTON_ICONS8_REGISTRY);

		if (bShow) {
			FILETERMGR().RemovFilter(emEventClass, emCMPIs, emRETExclude, GetClassStringMap(MONITOR_TYPE_REG));
		}else{
			FILETERMGR().AddFilter(emEventClass, emCMPIs, emRETExclude, GetClassStringMap(MONITOR_TYPE_REG));
		}

		CFltProcessDlg Dlg;
		Dlg.DoModal();

		m_view.SetItemCountEx((int)DATAVIEW().GetShowViewCounts(), 0);

		return 0;
	}

	LRESULT OnFileOpen(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		MessageBox(TEXT("TODO"));
		return 0;
	}

	LRESULT OnFilterClick(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		//MessageBox(TEXT("TODO"));
		CFilterDlg Dlg;
		Dlg.DoModal();
		return 0;
	}

	LRESULT OnScrollDownClick(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_SCROLLUP, FALSE);
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_SCROLLDOWN, TRUE);

		m_bScrollDown = TRUE;
		return 0;
	}

	LRESULT OnScrollUpClick(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_SCROLLUP, TRUE);
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_SCROLLDOWN, FALSE);

		m_bScrollDown = FALSE;
		return 0;
	}


	LRESULT OnMonitorStart(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_START, TRUE);
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_STOP, FALSE);
		MONITORMGR().Start();
		return 0;
	}

	LRESULT OnMonitorStop(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_START, FALSE);
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_STOP, TRUE);
		MONITORMGR().Stop();
		return 0;
	}


	LRESULT OnEraseShow(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		DATAVIEW().ClearShowViews();
		m_view.SetItemCountEx((int)DATAVIEW().GetShowViewCounts(), 0);
		return 0;
	}

	LRESULT OnEventProperties(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		CPropertiesDlg ProperiesDlg;
		ProperiesDlg.DoModal();
		return 0;
	}

	int GetProcessIconIndex(CRefPtr<CEventView> pEventView)
	{
		DWORD dwProcSeq = pEventView->GetProcessSeq();
		int nImageIndex = -1;
		auto it = m_ImageMap.find(dwProcSeq);
		if (it != m_ImageMap.end()) {
			nImageIndex = it->second;
		}else {

			//
			// get image and add to map
			//

			CBuffer& clsIconBuffer = pEventView->GetProcIcon(TRUE);

			if (!clsIconBuffer.Empty()) {
				
				//
				// Load from memory
				//

				HICON hIcon = CreateIconFromResourceEx(clsIconBuffer.GetBuffer(), 
					clsIconBuffer.GetBufferLen(), TRUE, 0x30000, 16, 16, 0);
				if (hIcon){
					nImageIndex = m_clsImageList.AddIcon(hIcon);
					DestroyIcon(hIcon);

					//
					// insert it
					//

					m_ImageMap.insert(std::make_pair(dwProcSeq, nImageIndex));
				}
			}
			
			if(nImageIndex == -1){
				nImageIndex = m_DefaultAppIcon;
				m_ImageMap.insert(std::make_pair(dwProcSeq, m_DefaultAppIcon));
			}
		}

		return nImageIndex;
	}

	LRESULT NotifyVDisplayHandler(int /*idCtrl*/, LPNMHDR pnmh, BOOL& bHandled)
	{
		NMLVDISPINFO* pDispInfo = reinterpret_cast<NMLVDISPINFO*>(pnmh);
		LVITEM* pItem = &pDispInfo->item;

		int iItem = pItem->iItem;

		CRefPtr<CEventView> pEventView = DATAVIEW().GetView(iItem);
		if (pEventView.IsNull()){
			return 0;
		}

		if (pItem->mask & LVIF_TEXT)
		{
			switch (pItem->iSubItem)
			{
				case 0:
					break;
				case 1: 	
				{
					//
					// Time of day
					//

					CString strTimeOfDay = UtilConvertTimeOfDay(pEventView->GetStartTime());
					StringCchCopy(pItem->pszText, pItem->cchTextMax, strTimeOfDay);
				}

					break;
				case 2: 
				{
					CString strProcessImage = pEventView->GetImagePath();

					if (pItem->mask & LVIF_IMAGE)
					{
						pItem->iImage = GetProcessIconIndex(pEventView);
					}

					StringCchCopy(pItem->pszText, pItem->cchTextMax, pEventView->GetProcessName());
				}
					break;
				case 3:
				{
					CString strPid;
					strPid.Format(TEXT("%d"), pEventView->GetProcessId());
					StringCchCopy(pItem->pszText, pItem->cchTextMax, strPid);
				}
					break;
				case 4:
				{
					DWORD dwClass = pEventView->GetEventClass();
					DWORD dwOperator = pEventView->GetEventOperator();

					CString strOperator;
					LPCTSTR lpOPt = GetOperatorStringMap(pEventView->GetPreEventEntry());
					if (!lpOPt){
						strOperator.Format(TEXT("%d:%d"), dwClass, dwOperator);
					}else{
						strOperator = lpOPt;
					}

					if (pItem->mask & LVIF_IMAGE)
					{
						switch (dwClass)
						{
						case MONITOR_TYPE_FILE:
							pItem->iImage = m_IconFile;
							break;
						case MONITOR_TYPE_PROCESS:
							pItem->iImage = m_IconProcess;
							break;
						case MONITOR_TYPE_REG:
							pItem->iImage = m_IconReg;
							break;
						default:
							break;
						}
					}

					StringCchCopy(pItem->pszText, pItem->cchTextMax, strOperator);
				}
					break;
				case 5:
				{
					StringCchCopy(pItem->pszText, pItem->cchTextMax, pEventView->GetPath());
				}
					break;
				case 6:
				{
					CString strResult;
					LPCTSTR lpDesc = StatusGetDesc(pEventView->GetResult());

					if (lpDesc){
						strResult = lpDesc;
					}else{
						strResult.Format(TEXT("0x%08x"), pEventView->GetResult());
					}
					
					StringCchCopy(pItem->pszText, pItem->cchTextMax, strResult);
				}
					break;
				case 7:
				{
					StringCchCopy(pItem->pszText, pItem->cchTextMax, pEventView->GetDetail());
				}
					break;
			}
		}
		return 0;
	}

	LRESULT NotifyItemChangedHandler(int /*idCtrl*/, LPNMHDR pnmh, BOOL& bHandled)
	{
		LPNMLISTVIEW pnmv = reinterpret_cast<LPNMLISTVIEW>(pnmh);
		if (pnmv->uNewState & LVIS_SELECTED){
			DATAVIEW().SetSelectIndex(pnmv->iItem);
		}
		return 0;
	}

	LRESULT NotifyRClickHandler(int /*idCtrl*/, LPNMHDR pnmh, BOOL& bHandled)
	{
		LPNMITEMACTIVATE pNMItemActivate = reinterpret_cast<LPNMITEMACTIVATE>(pnmh);

		if (pNMItemActivate->iItem != -1){

			//
			// Create Popup menu
			//
			
			CMenu clsMenu = CreatePopupMenu();
			
			//
			// Properties
			//
			
			clsMenu.AppendMenu(MF_STRING, ID_MEMU_PROPERTIES,TEXT("Properties..."));
			clsMenu.SetMenuDefaultItem(ID_MEMU_PROPERTIES);
			clsMenu.AppendMenu(MF_STRING, ID_MEMU_STACK, TEXT("Stack..."));
			clsMenu.AppendMenu(MF_STRING, ID_MEMU_BOOKMARK, TEXT("Toggle Bookmark"));
			clsMenu.AppendMenu(MF_STRING, ID_MEMU_BOOKMARK, TEXT("Jump to..."));

			clsMenu.AppendMenu(MF_SEPARATOR);
			
			//
			// Add Include exclude menu
			//
			
			TCHAR szColumName[260] = { 0 };
			LVCOLUMN Colum = {0};
			Colum.mask = LVCF_TEXT;
			Colum.pszText = szColumName;
			Colum.cchTextMax = 260;

			if (m_view.GetColumn(pNMItemActivate->iSubItem, &Colum)){
				CString strColumText = Colum.pszText;
				
				if (!strColumText.IsEmpty()){
					
					//
					// Get item text
					//

					CString strMenu;
					CString strItem;
					m_view.GetItemText(pNMItemActivate->iItem, pNMItemActivate->iSubItem, strItem);
					
					if (strItem.GetLength() > 50){
						strItem = strItem.Left(50);
						strItem += TEXT("...");
					}

					strMenu.Format(TEXT("Include \'%s\'"), strItem.GetBuffer());
					clsMenu.AppendMenu(MF_STRING, ID_MEMU_INCLUDE, strMenu);

					strMenu.Format(TEXT("HighLight \'%s\'"), strItem.GetBuffer());
					clsMenu.AppendMenu(MF_STRING, ID_MEMU_HIGHLIGHT, strMenu);

					strMenu.Format(TEXT("Exclude \'%s\'"), strItem.GetBuffer());
					clsMenu.AppendMenu(MF_STRING, ID_MEMU_EXCLUDE, strMenu);
				}
			}


			//
			// Show menu
			//

			DWORD dwPos = GetMessagePos();
			clsMenu.TrackPopupMenu(TPM_LEFTALIGN, LOWORD(dwPos), HIWORD(dwPos), this->m_hWnd);

		}

		return TRUE;
	}

	LRESULT OnNewOperator(UINT /*uMsg*/, WPARAM wParam, LPARAM /*lParam*/, BOOL& bHandled)
	{
		//m_view.SetItemCountEx((int)m_ShowViews.size(), LVSICF_NOINVALIDATEALL| LVSICF_NOSCROLL);
		bHandled = TRUE;
		return 0;
	}

	LRESULT OnCreate(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
	{
		//
		// create command bar window
		//

		HWND hWndCmdBar = m_CmdBar.Create(m_hWnd, rcDefault, NULL, ATL_SIMPLE_CMDBAR_PANE_STYLE);
		
		//
		// attach menu
		//

		m_CmdBar.AttachMenu(GetMenu());
		
		//
		// load command bar images
		//

		m_CmdBar.LoadImages(/*IDR_MAINFRAME*/IDR_TOOL);
		
		//
		// remove old menu
		//

		SetMenu(NULL);

		m_wndToolBar = CreateSimpleToolBarCtrl(m_hWnd, IDR_TOOL, FALSE, ATL_SIMPLE_TOOLBAR_PANE_STYLE);


		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_STOP, FALSE);
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_START, TRUE);

		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_SCROLLUP, TRUE);
		m_wndToolBar.HideButton(ID_BUTTON_ICONS8_SCROLLDOWN, FALSE);
		
		//
		// Set button style to check button
		//
		
		TBBUTTONINFO tbButtonInfo;
		tbButtonInfo.cbSize = sizeof(tbButtonInfo);
		tbButtonInfo.dwMask = TBIF_STYLE;
		m_wndToolBar.GetButtonInfo(ID_BUTTON_ICONS8_PROCESS, &tbButtonInfo);

		tbButtonInfo.fsStyle = BTNS_CHECK;
		m_wndToolBar.SetButtonInfo(ID_BUTTON_ICONS8_PROCESS, &tbButtonInfo);
		m_wndToolBar.SetButtonInfo(ID_BUTTON_ICONS8_REGISTRY, &tbButtonInfo);
		m_wndToolBar.SetButtonInfo(ID_BUTTON_ICONS8_FILE, &tbButtonInfo);

		m_wndToolBar.CheckButton(ID_BUTTON_ICONS8_PROCESS);
		m_wndToolBar.CheckButton(ID_BUTTON_ICONS8_FILE);

		//
		// set default all on
		//
		


		CreateSimpleReBar(ATL_SIMPLE_REBAR_NOBORDER_STYLE);
		AddSimpleReBarBand(hWndCmdBar);
		AddSimpleReBarBand(m_wndToolBar, NULL, TRUE);


		CreateSimpleStatusBar();
		DWORD SmallX = GetSystemMetrics(SM_CXSMICON);
		DWORD SmallY = GetSystemMetrics(SM_CYSMICON); 

		m_clsImageList.Create(SmallX, SmallY, 0xFF, 256, 256);

		m_hWndClient = m_view.Create(m_hWnd, rcDefault, NULL, WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | \
			WS_CLIPCHILDREN | LVS_REPORT | LVS_SHOWSELALWAYS | LVS_OWNERDATA, 
			WS_EX_CLIENTEDGE, IDC_LISTCTRL);
		
		//
		// Add column for list view
		//
		
		m_view.SetImageList(m_clsImageList, LVSIL_SMALL);
		m_view.SetExtendedListViewStyle(LVS_EX_FULLROWSELECT | LVS_EX_HEADERDRAGDROP | LVS_EX_SUBITEMIMAGES | LVS_EX_DOUBLEBUFFER);

		int n = 1;

		m_view.InsertColumn(0, TEXT("Fake"), 0, 0);
		m_view.InsertColumn(n++, TEXT("Time"), 0, 170);
		m_view.InsertColumn(n++, TEXT("Process Name"), 0, 280);
		m_view.InsertColumn(n++, TEXT("PID"), 0, 80);
		m_view.InsertColumn(n++, TEXT("Operation"), 0, 200);
		m_view.InsertColumn(n++, TEXT("Path"), 0, 380);	
		m_view.InsertColumn(n++, TEXT("Result"), 0, 180);
		m_view.InsertColumn(n++, TEXT("Detail"), 0, 180);

		HICON hDefault = UtilGetDefaultIcon(TRUE);
		if(hDefault){
			m_DefaultAppIcon = m_clsImageList.AddIcon(hDefault);
		}

		CIcon IcoProcess;
		IcoProcess.LoadIcon(IDI_ICON_PROCESS);
		m_IconProcess = m_clsImageList.AddIcon(IcoProcess);

		CIcon IcoFile;
		IcoFile.LoadIcon(IDI_ICON_FILE);
		m_IconFile = m_clsImageList.AddIcon(IcoFile);

		CIcon IcoReg;
		IcoReg.LoadIcon(IDI_ICON_REGISTERY);
		m_IconReg = m_clsImageList.AddIcon(IcoReg);

		UIAddToolBar(m_wndToolBar);
		UISetCheck(ID_VIEW_TOOLBAR, 1);
		UISetCheck(ID_VIEW_STATUS_BAR, 1);

		//
		// register object for message filtering and idle updates
		//

		CMessageLoop* pLoop = _Module.GetMessageLoop();
		ATLASSERT(pLoop != NULL);
		pLoop->AddMessageFilter(this);
		pLoop->AddIdleHandler(this);

		if (MONITORMGR().Connect()) {
			
			//
			// register call back
			//
			
			EVENTMGR().RegisterCallback(this);
			
			MONITORMGR().SetMonitor(TRUE, TRUE, FALSE);

			//
			// start
			//
			
			MONITORMGR().Start();
		}

		return 0;
	}

	LRESULT OnDestroy(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& bHandled)
	{
		//
		// unregister message filtering and idle updates
		//

		CMessageLoop* pLoop = _Module.GetMessageLoop();
		ATLASSERT(pLoop != NULL);
		pLoop->RemoveMessageFilter(this);
		pLoop->RemoveIdleHandler(this);

		bHandled = FALSE;

		MONITORMGR().Stop();
		MONITORMGR().Destory();

		return 1;
	}

	LRESULT OnFileExit(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		PostMessage(WM_CLOSE);
		return 0;
	}

	LRESULT OnFileNew(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		// TODO: add code to initialize document

		return 0;
	}

	LRESULT OnViewToolBar(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		static BOOL bVisible = TRUE;	// initially visible
		bVisible = !bVisible;
		CReBarCtrl rebar = m_hWndToolBar;
		int nBandIndex = rebar.IdToIndex(ATL_IDW_BAND_FIRST + 1);	// toolbar is 2nd added band
		rebar.ShowBand(nBandIndex, bVisible);
		UISetCheck(ID_VIEW_TOOLBAR, bVisible);
		UpdateLayout();
		return 0;
	}

	LRESULT OnViewStatusBar(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		BOOL bVisible = !::IsWindowVisible(m_hWndStatusBar);
		::ShowWindow(m_hWndStatusBar, bVisible ? SW_SHOWNOACTIVATE : SW_HIDE);
		UISetCheck(ID_VIEW_STATUS_BAR, bVisible);
		UpdateLayout();
		return 0;
	}

	LRESULT OnAppAbout(WORD /*wNotifyCode*/, WORD /*wID*/, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
	{
		CAboutDlg dlg;
		dlg.DoModal();
		return 0;
	}

	virtual BOOL DoEvent(CRefPtr<CEventView> pEventView)
	{
		DATAVIEW().Push(pEventView);
		return TRUE;
	}

private:
	CImageList m_clsImageList;
	std::map<DWORD, int> m_ImageMap;
	int m_DefaultAppIcon = 0;
	CToolBarCtrl m_wndToolBar;

	int m_IconProcess = 0;
	int m_IconFile = 0;
	int m_IconReg = 0;

	BOOL m_bScrollDown = FALSE;
};
