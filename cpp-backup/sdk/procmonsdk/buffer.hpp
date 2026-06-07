
#ifndef __BUFFER_INCLUDE__H__
#define __BUFFER_INCLUDE__H__


class CBuffer  
{

public:
	CBuffer();
	CBuffer(PBYTE pData, UINT nSize);
	CBuffer(const CBuffer& Src);
	virtual ~CBuffer();

	CBuffer& operator=(const CBuffer& Src);
	CBuffer& operator+=(const CBuffer& Src);
	bool operator==(const CBuffer& Src);

protected:
	UINT ReAllocateBuffer(UINT nRequestedSize);
	UINT DeAllocateBuffer(UINT nRequestedSize);
	UINT GetMemSize() const;

public:
	void ClearBuffer();
	UINT Delete(UINT nSize);
	UINT Read(PBYTE pData, UINT nSize);
	bool Write(PBYTE pData, UINT nSize);
	UINT GetBufferLen() const;
	BOOL Empty();
	bool Insert(PBYTE pData, UINT nSize);
	void Copy(const CBuffer& buffer);	
	PBYTE GetBuffer(UINT nPos=0) const;
	void Clear();

protected:
	PBYTE m_pBase;
	PBYTE m_pPtr;
	UINT m_nSize;

};

#endif
