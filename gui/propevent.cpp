

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

	CString strTmp;

	strTmp.Format(TEXT("%d"), pView->GetThreadId());
	GetDlgItem(1029).SetWindowText(strTmp);

	//strTmp.Format(TEXT("%d"), pView->GetEventClass());
	strTmp = GetClassStringMap(pView->GetEventClass());
	GetDlgItem(1030).SetWindowText(strTmp);

	//strTmp.Format(TEXT("%d"), pView->GetEventOperator());
	strTmp = GetOperatorStringMap(pView->GetPreEventEntry());
	GetDlgItem(1041).SetWindowText(strTmp);
	
	strTmp = StatusGetDesc(pView->GetResult());
	GetDlgItem(1042).SetWindowText(strTmp);

	GetDlgItem(1027).SetWindowText(pView->GetPath());
	
	strTmp.Format(TEXT("TODO %d"), 0);
	GetDlgItem(1028).SetWindowText(strTmp);
	
	GetDlgItem(1017).SetWindowText(pView->GetDetail());

	return 0;
}



