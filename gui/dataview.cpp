
#include "stdafx.h"
#include "dataview.h"
#include "filtermgr.h"

CDataView::CDataView()
{
	FILETERMGR().AddFilter(emPath, emCMPContains, emRETExclude, TEXT("$Extend"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$UpCase"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Secure"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$BadClus"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Boot"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Bitmap"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Root"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$AttrDef"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Volume"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$LogFile"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$MftMirr"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Mft"));
	FILETERMGR().AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("pagefile.sys"));
	FILETERMGR().AddFilter(emResult, emCMPEndWith, emRETExclude, TEXT("FAST_IO"));
	FILETERMGR().AddFilter(emOperation, emCMPBeginWith, emRETExclude, TEXT("FASTIO_"));
	FILETERMGR().AddFilter(emOperation, emCMPBeginWith, emRETExclude, TEXT("IRP_MJ_"));
	FILETERMGR().AddFilter(emProcessName, emCMPIs, emRETExclude, TEXT("system"));

	TCHAR szPath[MAX_PATH];
	GetModuleFileName(NULL, szPath, MAX_PATH);
	LPCTSTR lpAppName = PathFindFileName(szPath);

	FILETERMGR().AddFilter(emProcessName, emCMPIs, emRETExclude, lpAppName);
}

CDataView::~CDataView()
{

}

void CDataView::SetSelectIndex(size_t Index)
{
	m_SelectIndex = Index;
}

size_t CDataView::GetSelectIndex()
{
	return m_SelectIndex;
}

CRefPtr<CEventView> CDataView::GetSelectView()
{
	return GetView(m_SelectIndex);
}

CRefPtr<CEventView> CDataView::GetView(size_t Index)
{
	if (Index >= m_ShowViews.size()) {
		return NULL;
	}

	std::shared_lock<std::shared_mutex> lock(m_Viewlock);
	return m_ShowViews.at(Index);
}

size_t CDataView::GetShowViewCounts()
{
	return m_ShowViews.size();
}

void CDataView::ClearShowViews()
{
	std::unique_lock<std::shared_mutex> lock(m_Viewlock);
	m_ShowViews.clear();
}

void CDataView::Push(CRefPtr<CEventView> pOpt)
{
	
	//
	// do not process process init message
	//
	
	if (pOpt->GetEventClass() == MONITOR_TYPE_PROCESS &&
		pOpt->GetEventOperator() == NOTIFY_PROCESS_INIT){
		return;
	}

	if(!m_EopCheck.Check(pOpt)){
		return;
	}
	
	m_OptViewlock.lock();
	m_OptViews.push_back(pOpt);
	m_OptViewlock.unlock();

	//
	// Is filtered?
	//
	
	if (!FILETERMGR().Filter(pOpt)){
		m_Viewlock.lock();
		m_ShowViews.push_back(pOpt);
		m_Viewlock.unlock();
	}
}

void CDataView::ApplyNewFilter()
{
	ClearShowViews();
	
	std::shared_lock<std::shared_mutex> lock(m_OptViewlock);
	for (auto it = m_OptViews.begin(); it != m_OptViews.end(); it++)
	{
		if (!FILETERMGR().Filter(*it)){
			m_Viewlock.lock();
			m_ShowViews.push_back(*it);
			m_Viewlock.unlock();
		}
	}

}
