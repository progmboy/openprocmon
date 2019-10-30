

#include "pch.hpp"

#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#else
#define MoveMemory memmove
#define CopyMemory memcpy
#endif

#include "buffer.hpp"

#define ALIGN_SIZE 0x10

/** \fn CBuffer::CBuffer()
    \brief Constructs the buffer with a default size
*/

CBuffer::CBuffer()
{
	//
	// Initial size
	//

	m_nSize = 0;

	m_pPtr = m_pBase = NULL;
}

CBuffer::CBuffer(PBYTE pData, UINT nSize) :
	CBuffer()
{
	ClearBuffer();
	Write(pData, nSize);
}

CBuffer::CBuffer(const CBuffer& Src):
	CBuffer()
{
	Copy(Src);
}

/** \fn CBuffer::~CBuffer()
    \brief Deallocates the buffer
*/

CBuffer::~CBuffer()
{
	if (m_pBase)
		HeapFree(GetProcessHeap(), 0, m_pBase);//free(m_pBase);
}

bool CBuffer::operator==(const CBuffer& Src)
{
	if (this->GetBufferLen() != Src.GetBufferLen()){
		return false;
	}

	return memcmp(this->GetBuffer(), Src.GetBuffer(), this->GetBufferLen()) == 0;
}

CBuffer& CBuffer::operator+=(const CBuffer& Src)
{
	Write(Src.GetBuffer(), Src.GetBufferLen());
	return(*this);
}

CBuffer& CBuffer::operator=(const CBuffer& Src)
{
	Copy(Src);
	return(*this);
}

/** \fn BOOL CBuffer::Write(PBYTE pData, UINT nSize)
    \brief Writes data into the buffer
    \return 
*/

bool CBuffer::Write(PBYTE pData, UINT nSize)
{
	if(!ReAllocateBuffer(nSize + GetBufferLen())){
		return false;
	}

	CopyMemory(m_pPtr, pData, nSize);

	//
	// Advance Pointer
	//

	m_pPtr += nSize;

	return nSize != 0;
}

/** \fn BOOL CBuffer::Insert(PBYTE pData, UINT nSize)
    \brief Insert data into the buffer 
    \return 
*/

bool CBuffer::Insert(PBYTE pData, UINT nSize)
{
	ReAllocateBuffer(nSize + GetBufferLen());

	MoveMemory(m_pBase + nSize, m_pBase, GetMemSize() - nSize);
	CopyMemory(m_pBase, pData, nSize);

	//
	// Advance Pointer
	//

	m_pPtr += nSize;

	return nSize != 0;
}

/** \fn UINT CBuffer::Read(PBYTE pData, UINT nSize)
    \brief Reads data from the buffer and deletes what it reads
    \return 
*/

UINT CBuffer::Read(PBYTE pData, UINT nSize)
{
	//
	// Trying to byte off more than ya can chew - eh?
	//

	if (nSize > GetMemSize())
		return 0;

	//
	// all that we have 
	//

	if (nSize > GetBufferLen())
		nSize = GetBufferLen();

		
	if (nSize){

		//
		// Copy over required amount and its not up to us
		// to terminate the buffer - got that!!!
		//

		CopyMemory(pData,m_pBase,nSize);
		
		//
		// Slide the buffer back - like sinking the data
		//

		MoveMemory(m_pBase,m_pBase+nSize,GetMemSize() - nSize);

		m_pPtr -= nSize;
	}
		
	DeAllocateBuffer(GetBufferLen());

	return nSize;
}

/** \fn UINT CBuffer::GetMemSize() 
    \brief Returns the physical memory allocated to the buffer
    \return 
*/

UINT CBuffer::GetMemSize() const
{
	return m_nSize;
}

/** \fn UINT CBuffer::GetBufferLen() 
	\brief Get the buffer 'data' length
    \return 
*/

UINT CBuffer::GetBufferLen() const
{
	if (m_pBase == NULL)
		return 0;

	return (UINT)((ULONG_PTR)m_pPtr - (ULONG_PTR)m_pBase);
}

BOOL CBuffer::Empty()
{
	return GetBufferLen() == 0;
}


#define ALIGN_DOWN_BY(length, alignment) \
    ((ULONG_PTR)(length) & ~((ULONG_PTR)(alignment) - 1))

#define ALIGN_UP_BY(length, alignment) \
    (ALIGN_DOWN_BY(((ULONG_PTR)(length) + (alignment) - 1), alignment))

#define ALIGN_DOWN(length, type) \
    ALIGN_DOWN_BY(length, sizeof(type))

#define ALIGN_UP(length, type) \
    ALIGN_UP_BY(length, sizeof(type))

/** \fn UINT CBuffer::ReAllocateBuffer(UINT nRequestedSize)
    \brief ReAllocateBuffer the Buffer to the requested size
    \return 
*/

UINT CBuffer::ReAllocateBuffer(UINT nRequestedSize)
{
	if (nRequestedSize < GetMemSize())
		return 0;

	//
	// Allocate new size
	//

	UINT nNewSize = ALIGN_UP_BY(nRequestedSize, ALIGN_SIZE);

	//
	// New Copy Data Over
	//

	PBYTE pNewBuffer = (PBYTE)HeapAlloc(GetProcessHeap(), 0, nNewSize); //malloc(nNewSize);
	if (!pNewBuffer){
		return 0;
	}

	UINT nBufferLen = GetBufferLen();
	CopyMemory(pNewBuffer, m_pBase, nBufferLen);

	if (m_pBase)
		HeapFree(GetProcessHeap(), 0, m_pBase);//free(m_pBase);

	//
	// Hand over the pointer
	//

	m_pBase = pNewBuffer;

	//
	// Realign position pointer
	//

	m_pPtr = m_pBase + nBufferLen;

	m_nSize = nNewSize;

	return m_nSize;
}

/** \fn UINT CBuffer::DeAllocateBuffer(UINT nRequestedSize)
    \brief DeAllocates the Buffer to the requested size
    \return 
*/

UINT CBuffer::DeAllocateBuffer(UINT nRequestedSize)
{
	if (nRequestedSize < GetBufferLen())
		return 0;

	//
	// Allocate new size
	//

	UINT nNewSize = ALIGN_UP_BY(nRequestedSize, ALIGN_SIZE);

	if (nNewSize < GetMemSize())
		return 0;

	//
	// New Copy Data Over
	//

	PBYTE pNewBuffer = (PBYTE)HeapAlloc(GetProcessHeap(), 0, nNewSize);//malloc(nNewSize);
	if (!pNewBuffer){
		return 0;
	}

	UINT nBufferLen = GetBufferLen();
	CopyMemory(pNewBuffer,m_pBase,nBufferLen);

	HeapFree(GetProcessHeap(), 0, m_pBase);//free(m_pBase);

	//
	// Hand over the pointer
	//

	m_pBase = pNewBuffer;

	//
	// Realign position pointer
	//

	m_pPtr = m_pBase + nBufferLen;

	m_nSize = nNewSize;

	return m_nSize;
}


/** \fn void CBuffer::ClearBuffer()
    \brief Clears/Resets the buffer
    \return 
*/

void CBuffer::ClearBuffer()
{
	//
	// Force the buffer to be empty
	//

	m_pPtr = m_pBase;

	DeAllocateBuffer(ALIGN_SIZE);
}

/** \fn void CBuffer::Copy(CBuffer& buffer)
    \brief Copy from one buffer object to another...
    \return 
*/

void CBuffer::Copy(const CBuffer& buffer)
{
	int nReSize = buffer.GetMemSize();
	int nSize = buffer.GetBufferLen();
	ClearBuffer();
	ReAllocateBuffer(nReSize);

	m_pPtr = m_pBase + nSize;

	CopyMemory(m_pBase,buffer.GetBuffer(),buffer.GetBufferLen());
}

/** \fn PBYTE CBuffer::GetBuffer(UINT nPos)
    \brief Returns a pointer to the physical memory determined by the offset
    \return 
*/

PBYTE CBuffer::GetBuffer(UINT nPos) const
{
	return m_pBase+nPos;
}


void CBuffer::Clear()
{
	if (m_pBase) {
		HeapFree(GetProcessHeap(), 0, m_pBase);
	}

	m_nSize = 0;
	m_pBase = m_pPtr = NULL;
}

/** \fn UINT CBuffer::Delete(UINT nSize)
    \brief Delete data from the buffer and deletes what it reads
    \return 
*/

UINT CBuffer::Delete(UINT nSize)
{
	//
	// Trying to byte off more than ya can chew - eh?
	//

	if (nSize > GetMemSize())
		return 0;

	//
	// all that we have 
	//

	if (nSize > GetBufferLen())
		nSize = GetBufferLen();

		
	if (nSize){

		//
		// Slide the buffer back - like sinking the data
		//

		MoveMemory(m_pBase,m_pBase+nSize,GetMemSize() - nSize);

		m_pPtr -= nSize;
	}
		
	DeAllocateBuffer(GetBufferLen());

	return nSize;
}