
#include "stdafx.h"
#include "resource.h"
#include "dataview.h"
#include "filterdlg.h"

#define WM_CUSTOM_FILTER	(WM_USER+2)

LPCTSTR gSourceTypeStr[] = {
	TEXT("Architeture"),
	TEXT("AuthId"),
	TEXT("Category"),
	TEXT("CommandLine"),
	TEXT("Company"),
	TEXT("CompletionTime"),
	TEXT("DataTime"),
	TEXT("Description"),
	TEXT("Detail"),
	TEXT("Duration"),
	TEXT("EventClass"),
	TEXT("ImagePath"),
	TEXT("Integrity"),
	TEXT("Operation"),
	TEXT("ParentPid"),
	TEXT("Path"),
	TEXT("PID"),
	TEXT("ProcessName"),
	TEXT("RelativeTime"),
	TEXT("Result"),
	TEXT("Sequence"),
	TEXT("Session"),
	TEXT("TID"),
	TEXT("TimeOfDay"),
	TEXT("User"),
	TEXT("Version"),
	TEXT("Virtualize")
};

LPCTSTR gCmpTypeStr[] = {

	TEXT("Is"),
	TEXT("Is Not"),
	TEXT("Less Than"),
	TEXT("More Than"),
	TEXT("Begin With"),
	TEXT("End With"),
	TEXT("Contains"),
	TEXT("Excludes")
};

LPCTSTR gRetTypeStr[] = {
	TEXT("Include"),
	TEXT("Exclude")
};


LRESULT CFilterDlg::OnInitDialog(UINT uMsg, WPARAM wParam, LPARAM lParam, BOOL& bHandled)
{
	DlgResize_Init(false);

	m_ComboBoxSrc = GetDlgItem(IDC_FILTER_SRC);
	m_ComboBoxOpt = GetDlgItem(IDC_FILTER_OPT);
	m_ComboBoxDst = GetDlgItem(IDC_FILTER_DEST);
	m_ComboBoxRet = GetDlgItem(IDC_FILTER_RET);
	m_FilterListView = this->GetDlgItem(IDC_FILTER_LIST);

	m_ApplyBtn = GetDlgItem(IDC_FILTER_APPLY);
	m_ApplyBtn.EnableWindow(FALSE);

	for (int i = 0; i < _countof(gSourceTypeStr); i++)
	{
		m_ComboBoxSrc.AddString(gSourceTypeStr[i]);
	}

	for (int i = 0; i < _countof(gCmpTypeStr); i++)
	{
		m_ComboBoxOpt.AddString(gCmpTypeStr[i]);
	}

	for (int i = 0; i < _countof(gRetTypeStr); i++)
	{
		m_ComboBoxRet.AddString(gRetTypeStr[i]);
	}

	m_ComboBoxSrc.SetCurSel(0);
	m_ComboBoxOpt.SetCurSel(0);
	m_ComboBoxRet.SetCurSel(0);

	DWORD SmallX = GetSystemMetrics(SM_CXSMICON);
	DWORD SmallY = GetSystemMetrics(SM_CYSMICON);

	m_clsImageList.Create(SmallX, SmallY, 0xFF, 256, 256);

	CIcon IcoInclude;
	IcoInclude.LoadIcon(IDI_ICON_ENABLE);
	m_IcoRet[0] = m_clsImageList.AddIcon(IcoInclude);

	CIcon IcoExclude;
	IcoExclude.LoadIcon(IDI_ICON_DISABLE);
	m_IcoRet[1] = m_clsImageList.AddIcon(IcoExclude);

	m_FilterListView.SetExtendedListViewStyle(LVS_EX_FULLROWSELECT | LVS_EX_CHECKBOXES);
	m_FilterListView.InsertColumn(0, TEXT("Column"), LVCFMT_LEFT, 300);
	m_FilterListView.InsertColumn(1, TEXT("Relation"), LVCFMT_LEFT, 150);
	m_FilterListView.InsertColumn(2, TEXT("Value"), LVCFMT_LEFT, 300);
	m_FilterListView.InsertColumn(3, TEXT("Action"), LVCFMT_LEFT, 150);
	m_FilterListView.SetImageList(m_clsImageList, LVSIL_SMALL);

	auto FilterList = DATAVIEW().GetFilterMgr().GetFilterList();

	int nIndex = 0;
	for (auto filter : FilterList)
	{
		int SrcIndex = static_cast<int>(filter->GetSourceType());
		int CmpIndex = static_cast<int>(filter->GetCmpType());
		int RetIndex = static_cast<int>(filter->GetRetType());
		auto& strDst = filter->GetFilter();

		m_FilterListView.InsertItem(nIndex, gSourceTypeStr[SrcIndex], m_IcoRet[RetIndex]);
		m_FilterListView.SetItemText(nIndex, 1, gCmpTypeStr[CmpIndex]);
		m_FilterListView.SetItemText(nIndex, 2, strDst);
		m_FilterListView.SetItemText(nIndex, 3, gRetTypeStr[RetIndex]);

		m_FilterListView.SetCheckState(nIndex, filter->IsEnable());

		nIndex++;
	}

	return 0;
}

int CFilterDlg::SourceTypeStringToIndex(const CString& strValue)
{
	for (int i = 0; i < _countof(gSourceTypeStr); i++)
	{
		if (strValue == gSourceTypeStr[i]) {
			return i;
		}
	}

	return 0;
}

int CFilterDlg::CmpTypeStringToIndex(const CString& strValue)
{
	for (int i = 0; i < _countof(gCmpTypeStr); i++)
	{
		if (strValue == gCmpTypeStr[i]) {
			return i;
		}
	}

	return 0;
}

int CFilterDlg::RetTypeStringToIndex(const CString& strValue)
{
	for (int i = 0; i < _countof(gRetTypeStr); i++)
	{
		if (strValue == gRetTypeStr[i]) {
			return i;
		}
	}

	return 0;
}

LRESULT CFilterDlg::OnBtnAdd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
{
	int RetIndex = m_ComboBoxRet.GetCurSel();
	int SrcIndex = m_ComboBoxSrc.GetCurSel();
	int CmpIndex = m_ComboBoxOpt.GetCurSel();
	CString strFilter;
	m_ComboBoxDst.GetWindowText(strFilter);

	m_FilterListView.InsertItem(0, gSourceTypeStr[SrcIndex], m_IcoRet[RetIndex]);
	m_FilterListView.SetItemText(0, 1, gCmpTypeStr[CmpIndex]);
	m_FilterListView.SetItemText(0, 2, strFilter);
	m_FilterListView.SetItemText(0, 3, gRetTypeStr[RetIndex]);

	m_FilterListView.SetCheckState(0, TRUE);

	m_ApplyBtn.EnableWindow(TRUE);
	return S_OK;
}

LRESULT CFilterDlg::OnApplyCmd(WORD /*wNotifyCode*/, WORD wID, HWND /*hWndCtl*/, BOOL& /*bHandled*/)
{
	DATAVIEW().GetFilterMgr().RemoveAll();
	int nCounts = m_FilterListView.GetItemCount();

	for (auto i = 0; i < nCounts; i++)
	{
		CString strTemp;
		m_FilterListView.GetItemText(i, 0, strTemp);
		auto srcType = static_cast<MAP_SOURCE_TYPE>(SourceTypeStringToIndex(strTemp));

		m_FilterListView.GetItemText(i, 1, strTemp);
		auto optType = static_cast<FILTER_CMP_TYPE>(CmpTypeStringToIndex(strTemp));

		CString strFilter;
		m_FilterListView.GetItemText(i, 2, strFilter);

		m_FilterListView.GetItemText(i, 3, strTemp);
		auto retType = static_cast<FILTER_RESULT_TYPE>(RetTypeStringToIndex(strTemp));

		BOOL bEnable = m_FilterListView.GetCheckState(i);

		DATAVIEW().GetFilterMgr().AddFilter(srcType, optType, retType, strFilter, bEnable);
	}

	GetParent().PostMessage(WM_CUSTOM_FILTER, 0, 0);

	return 0;
}