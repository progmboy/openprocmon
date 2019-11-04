

#include "stdafx.h"
#include "resource.h"
#include "dataview.h"
#include "status.h"
#include "propevent.h"

LRESULT CPropEventDlg::OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
{
	CRefPtr<CEventView> pView = DATAVIEW().GetSelectView();
	if (pView.IsNull()){
		return 0;
	}

	CString strTime = UtilConvertTimeOfDay(pView->GetStartTime());

	GetDlgItem(IDC_EVENT_DATA).SetWindowText(strTime);
	GetDlgItem(IDC_EVENT_THREAD).SetWindowText(MapMonitorResult(emTID, pView));
	GetDlgItem(IDC_EVENT_CLASS).SetWindowText(MapMonitorResult(emEventClass, pView));
	GetDlgItem(IDC_EVENT_OPT).SetWindowText(MapMonitorResult(emOperation, pView));
	GetDlgItem(IDC_EVENT_RET).SetWindowText(MapMonitorResult(emResult, pView));
	GetDlgItem(IDC_EVENT_PATH).SetWindowText(pView->GetPath());
	
	//
	// Duration
	//
	
	GetDlgItem(IDC_EVENT_DURATION).SetWindowText(MapMonitorResult(emDuration, pView));
	GetDlgItem(IDC_EVENT_DETAIL).SetWindowText(pView->GetDetail());

	return 0;
}



