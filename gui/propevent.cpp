

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

	GetDlgItem(1040).SetWindowText(strTime);

	GetDlgItem(1029).SetWindowText(MapMonitorResult(emTID, pView));

	GetDlgItem(1030).SetWindowText(MapMonitorResult(emEventClass, pView));

	GetDlgItem(1041).SetWindowText(MapMonitorResult(emOperation, pView));
	GetDlgItem(1042).SetWindowText(MapMonitorResult(emResult, pView));

	GetDlgItem(1027).SetWindowText(pView->GetPath());
	
	
	//
	// Duration
	//
	
	GetDlgItem(1028).SetWindowText(MapMonitorResult(emDuration, pView));
	
	GetDlgItem(1017).SetWindowText(pView->GetDetail());

	return 0;
}



