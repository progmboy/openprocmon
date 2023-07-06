
#include "stdafx.h"
#include "dataview.h"
#include "filtermgr.h"

CDataView::CDataView()
{
	m_Filter.AddFilter(emPath, emCMPContains, emRETExclude, TEXT("$Extend"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$UpCase"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Secure"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$BadClus"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Boot"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Bitmap"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Root"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$AttrDef"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Volume"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$LogFile"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$MftMirr"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("$Mft"));
	m_Filter.AddFilter(emPath, emCMPEndWith, emRETExclude, TEXT("pagefile.sys"));
	m_Filter.AddFilter(emResult, emCMPEndWith, emRETExclude, TEXT("FAST_IO"));
	m_Filter.AddFilter(emOperation, emCMPBeginWith, emRETExclude, TEXT("FASTIO_"));
	m_Filter.AddFilter(emOperation, emCMPBeginWith, emRETExclude, TEXT("IRP_MJ_"));
	m_Filter.AddFilter(emProcessName, emCMPIs, emRETExclude, TEXT("system"));

	//m_Filter.AddFilter(emProcessName, emCMPIs, emRETInclude, TEXT("notepad.exe"));
	//m_Filter.AddFilter(emProcessName, emCMPIs, emRETInclude, TEXT("pdbex.exe"));

	TCHAR szPath[MAX_PATH];
	GetModuleFileName(NULL, szPath, MAX_PATH);
	LPCTSTR lpAppName = PathFindFileName(szPath);

	m_Filter.AddFilter(emProcessName, emCMPIs, emRETExclude, lpAppName);
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

BOOL CDataView::IsHighlight(size_t Index)
{
#if 0
	CRefPtr<CEventViewExt> pExt = _Get(Index);
	if (!pExt.IsNull())
		return pExt->IsHighLight();
	else
		return FALSE;
#endif

	CRefPtr<CEventViewExt> pExt = _Get(Index);
	if (!pExt.IsNull()) {
		if (pExt->IsHighLight()){
			return TRUE;
		}else{
			
			//
			// check highlight filter
			//

			if(m_HighLightFilter.GetCounts()){
				if (!m_HighLightFilter.Filter(pExt->GetView())) {
					return TRUE;
				}
			}
		}
	}	
	return FALSE;
}

CRefPtr<CEventView> CDataView::GetSelectView()
{
	return GetView(m_SelectIndex);
}

CRefPtr<CEventView> CDataView::GetView(size_t Index)
{
	CRefPtr<CEventViewExt> pExt = _Get(Index);
	if (!pExt.IsNull())
		return pExt->GetView();
	else
		return NULL;
}

CRefPtr<CEventViewExt> CDataView::_Get(size_t Index)
{
	std::shared_lock<std::shared_mutex> lock(m_Viewlock);
	if (Index >= m_ShowViews.size()) {
		return NULL;
	}

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


	BOOL bHighLight = FALSE;
	CRefPtr<CEventViewExt> pOptEx = new CEventViewExt(pOpt, bHighLight);
	
	m_OptViewlock.lock();
	m_OptViews.push_back(pOptEx);
	m_OptViewlock.unlock();

	//
	// Is filtered?
	//
	
	if (!m_Filter.Filter(pOpt)){

		m_Viewlock.lock();
		m_ShowViews.push_back(pOptEx);
		m_Viewlock.unlock();
	}
}

void CDataView::AddFilter(CRefPtr<CFilter> pFilter)
{
	m_Filter.AddFilter(pFilter);
}

void CDataView::AddHighLightFilter(CRefPtr<CFilter> pFilter)
{
	m_HighLightFilter.AddFilter(pFilter);
}

void CDataView::RemoveFilter(CRefPtr<CFilter> pFilter)
{
	m_Filter.RemovFilter(pFilter);
}

void CDataView::RemoveHighLightFilter(CRefPtr<CFilter> pFilter)
{
	m_HighLightFilter.RemovFilter(pFilter);
}

void CDataView::ApplyNewFilter(FLTPROCGRESSCB Callback, LPVOID pParameter)
{
	ClearShowViews();
	
	std::unique_lock<std::shared_mutex> lock(m_OptViewlock);

	size_t Total = m_OptViews.size();
	size_t Now = 0;

	if (Callback) {
		Callback(Total, Now, pParameter);
	}

	for (auto it = m_OptViews.begin(); it != m_OptViews.end(); it++, Now++)
	{
		if (!m_Filter.Filter((*it)->GetView())){
			m_Viewlock.lock();
			m_ShowViews.push_back(*it);
			m_Viewlock.unlock();

			if (Callback){
				Callback(Total, Now, pParameter);
			}
		}
	}

}
