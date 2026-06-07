#ifndef __REF_OBJECT_H__
#define __REF_OBJECT_H__

#ifdef _WIN32
#include <windows.h>
#endif


class CRefBase
{
public:
	CRefBase()
	{
		m_nRefCount = 0;
	}

	virtual ~CRefBase()
	{

	}

	int GetRef() const
	{
		return m_nRefCount;
	}

	int AddRef()
	{
		return InterlockedIncrement((long*)&m_nRefCount);
	}

	virtual int DeRef()
	{
		long refCounts = InterlockedDecrement((long*)&m_nRefCount);
        return refCounts;
	}

	void Reset()
	{
		InterlockedCompareExchange((long*)&m_nRefCount, 0, m_nRefCount);
	}


private:
	int	m_nRefCount;
};

template<typename T>
class CRefPtr
{
public:
	T* operator->() const
	{
		return m_pRawObj;
	}

	T& operator()() const
	{
		return *m_pRawObj;
	}

	T& operator*() const
	{
		return *m_pRawObj;
	}

	T* GetPtr() const
	{
		return m_pRawObj;
	}

	bool IsNull() const
	{
		return m_pRawObj == NULL;
	}

	CRefPtr()
	{
		m_pRawObj = NULL;
	}

	CRefPtr(T* p)
	{
		m_pRawObj = p;
		if(p != NULL){
			p->AddRef();
		}
	}

	CRefPtr(const CRefPtr& ref)
	{
		m_pRawObj = ref.m_pRawObj;
		if(m_pRawObj != NULL){
			m_pRawObj->AddRef();
		}
	}

	~CRefPtr()
	{
		if(m_pRawObj != NULL && !m_pRawObj->DeRef()){

			//
			//delete obejct
			//
			
			//LogMessage(L_INFO, "refcounts == 0 delete it");
			delete m_pRawObj;
		}
	}

	CRefPtr& operator = (const CRefPtr& ref)
	{
		if(this != &ref){
			if(m_pRawObj != NULL && !m_pRawObj->DeRef()){
				delete m_pRawObj;
			}

			m_pRawObj = ref.m_pRawObj;

			if(m_pRawObj != NULL){
				m_pRawObj->AddRef();
			}
		}

		return *this;
	}

	bool operator == (const CRefPtr& ref) const
	{
		return m_pRawObj == ref.m_pRawObj;
	}

	bool operator != (const CRefPtr& ref) const
	{
		return m_pRawObj != ref.m_pRawObj;
	}

private:
	T* m_pRawObj;
};

#endif //__REF_OBJECT_H__
