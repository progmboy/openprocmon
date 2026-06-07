
#include "pch.hpp"

#include <stdlib.h>
#include <string.h>
#include <signal.h>
#include <time.h>
#include <windows.h>

#include "thread.hpp"
#include "logger.hpp"

CThread::CThread()
{
}

CThread::~CThread()
{
    //Kill();
	SetTimeout(5);
	Stop();
}

DWORD WINAPI CThread::ThreadProc(void* lpParam)
{
    CThread* pThread = (CThread*)lpParam;
    pThread->m_startTime = time(NULL);
    pThread->Run();
    pThread->m_bIsStart = false;
    pThread->m_startTime = 0;
    return NULL;
}

bool CThread::Start()
{
	if (IsAlive()){
		return true;
	}else {
		m_bIsStart = true;
		m_bIsStop = false;
		m_ThreadHandle = CreateThread(NULL, 0, CThread::ThreadProc, this, 0, &m_ThreadID);
		return true;
	}
}


void CThread::Stop()
{
	m_bIsStop = true;
	//m_bIsStart = false;
	TryJoin();
}

void CThread::Join()
{
	WaitForSingleObject(m_ThreadHandle, INFINITE);
}

DWORD CThread::GetThreadID()
{
    return m_ThreadID;
}

void CThread::TryJoin()
{
    if(!m_bIsStart)
        return;
    
    if(!m_TimeoutSeconds){
        return Join();
    }


    int err = WaitForSingleObject(m_ThreadHandle, m_TimeoutSeconds*1000);
    if(err == WAIT_TIMEOUT){
        
        //
        // wait timeout
        //
        
        LogMessage(L_INFO, TEXT("waitout kill it"));
        Kill();
	} else if (err == WAIT_OBJECT_0) {
		LogMessage(L_INFO, TEXT("wait success"));
    }else if(err == 0){
		LogMessage(L_INFO, TEXT("invalid wait"));
    }
}

void CThread::Kill()
{
    if(IsAlive()){
		TerminateThread(m_ThreadHandle, -1);
    } 
    m_bIsStart = false;
    m_startTime = 0;
}

bool CThread::IsAlive()
{
    return m_bIsStart;
}

void CThread::SetTimeout(uint32_t timeout)
{
    m_TimeoutSeconds = timeout;
}

bool CThread::IsTimeout()
{
    if(!m_startTime || !m_TimeoutSeconds || !IsAlive()){
        return false;
    }

    return (time(NULL) - m_startTime) > m_TimeoutSeconds;
}