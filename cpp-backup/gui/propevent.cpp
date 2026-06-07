

#include "stdafx.h"
#include "resource.h"
#include "dataview.h"
#include "propevent.h"

LRESULT CPropEventDlg::OnInitDialog(UINT /*uMsg*/, WPARAM /*wParam*/, LPARAM /*lParam*/, BOOL& /*bHandled*/)
{
	CRefPtr<CEventView> pView = DATAVIEW().GetSelectView();
	if (pView.IsNull()){
		return 0;
	}

	CString strTime = UtilConvertTimeOfDay(pView->GetStartTime());

	GetDlgItem(IDC_EVENT_DATA).SetWindowText(strTime);
	GetDlgItem(IDC_EVENT_THREAD).SetWindowText(pView->GetOperationStrResult(emTID));
	GetDlgItem(IDC_EVENT_CLASS).SetWindowText(pView->GetOperationStrResult(emEventClass));
	GetDlgItem(IDC_EVENT_OPT).SetWindowText(pView->GetOperationStrResult(emOperation));
	GetDlgItem(IDC_EVENT_RET).SetWindowText(pView->GetOperationStrResult(emResult));
	GetDlgItem(IDC_EVENT_PATH).SetWindowText(pView->GetPath());
	
	//
	// Duration
	//
	
	GetDlgItem(IDC_EVENT_DURATION).SetWindowText(pView->GetOperationStrResult(emDuration));
	GetDlgItem(IDC_EVENT_DETAIL).SetWindowText(pView->GetDetail());

	return 0;
}


CString CPropEventDlg::CopyAll()
{
	CString strCopy;
	CString strTemp;
	CString strItem;

	GetDlgItemText(IDC_EVENT_DATA, strItem);
	strTemp.Format(TEXT("Date: %s\n"), strItem);
	strCopy += strTemp;

	GetDlgItemText(IDC_EVENT_THREAD, strItem);
	strTemp.Format(TEXT("Thread: %s\n"), strItem);
	strCopy += strTemp;

	GetDlgItemText(IDC_EVENT_CLASS, strItem);
	strTemp.Format(TEXT("Event Class: %s\n"), strItem);
	strCopy += strTemp;

	GetDlgItemText(IDC_EVENT_OPT, strItem);
	strTemp.Format(TEXT("Operation: %s\n"), strItem);
	strCopy += strTemp;

	GetDlgItemText(IDC_EVENT_RET, strItem);
	strTemp.Format(TEXT("Result: %s\n"), strItem);
	strCopy += strTemp;

	GetDlgItemText(IDC_EVENT_PATH, strItem);
	strTemp.Format(TEXT("Path: %s\n"), strItem);
	strCopy += strTemp;

	GetDlgItemText(IDC_EVENT_DURATION, strItem);
	strTemp.Format(TEXT("Duration: %s\n"), strItem);
	strCopy += strTemp;

	GetDlgItemText(IDC_EVENT_DETAIL, strItem);
	strTemp.Format(TEXT("Detail: %s\n"), strItem);
	strCopy += strTemp;

	return strCopy;
}


