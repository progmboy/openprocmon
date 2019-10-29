
#include "stdafx.h"
#include "dataview.h"

CDataView::CDataView()
{

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

CRefPtr<COptView> CDataView::GetSelectView()
{
	return GetView(m_SelectIndex);
}

CRefPtr<COptView> CDataView::GetView(size_t Index)
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

void CDataView::Push(CRefPtr<COptView> pOpt)
{
	m_OptViews.push_back(pOpt);
	
	m_Viewlock.lock();
	m_ShowViews.push_back(pOpt);
	m_Viewlock.unlock();
}
