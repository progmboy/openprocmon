#pragma once

#include <vector>
#include <shared_mutex>
#include "filtermgr.h"

#define DATAVIEW()  Singleton<CDataView>::getInstance()

typedef VOID (*FLTPROCGRESSCB)(size_t Total, size_t Current, PVOID pParameter);

class CEventViewExt : public CRefBase
{
public:
	CEventViewExt(CRefPtr<CEventView> pOpt):
		m_EventView(pOpt),
		m_bHighLight(FALSE)
	{
	
	}

	CEventViewExt(CRefPtr<CEventView> pOpt, BOOL bH) :
		m_EventView(pOpt),
		m_bHighLight(bH)
	{

	}

	~CEventViewExt()
	{
	
	}

	VOID SetHighLight(BOOL bHighLigt)
	{
		m_bHighLight = bHighLigt;
	}

	BOOL IsHighLight()
	{
		return m_bHighLight;
	}

	CRefPtr<CEventView> GetView()
	{
		return m_EventView;
	}

private:
	CRefPtr<CEventView> m_EventView;
	BOOL m_bHighLight;
};

class CDataView
{
public:
	CDataView();
	~CDataView();

public:

	void SetSelectIndex(size_t Index);
	size_t GetSelectIndex();
	BOOL IsHighlight(size_t Index);
	CRefPtr<CEventView> GetSelectView();
	CRefPtr<CEventView> GetView(size_t Index);
	CRefPtr<CEventViewExt> _Get(size_t Index);
	size_t GetShowViewCounts();
	void ClearShowViews();
	void Push(CRefPtr<CEventView> pOpt);
	void AddFilter(CRefPtr<CFilter> pFilter);
	void AddHighLightFilter(CRefPtr<CFilter> pFilter);
	void RemoveFilter(CRefPtr<CFilter> pFilter);
	void RemoveHighLightFilter(CRefPtr<CFilter> pFilter);
	void ApplyNewFilter(FLTPROCGRESSCB Callback=NULL, LPVOID pParameter = NULL);

private:

	size_t m_SelectIndex = 0;

	//
	// 这里保存了所有的消息
	//

	std::vector<CRefPtr<CEventViewExt>> m_OptViews;

	//
	// 这里只保存需要显示的消息
	//

	std::vector<CRefPtr<CEventViewExt>> m_ShowViews;
	std::shared_mutex m_Viewlock;
	std::shared_mutex m_OptViewlock;
	CFilterMgr m_Filter;
	CFilterMgr m_HighLightFilter;
};