import { createContext, useContext, useState, useEffect, useCallback, ReactNode, useRef } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
import { basicAuthHeader } from '../lib/utils';

const API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';

interface Agent { client_id: string; device_name: string; publicKey: string; isVerified?: boolean; }
interface AuditLog { id: number, event_type: string, detail: string, severity: string, created_at: string }
interface ScheduledTask { id: number, task_name: string, cron_expression: string, created_at: string }
interface Permission { key: string, value: boolean }
interface InboxFile { id: number, file_name: string, sender: string, created_at: string }
interface KnowledgeDir { id: number, directory_path: string, file_count: number, created_at: string }
interface Group { id: string, name: string, creator_id: string, member_ids: string[], created_at: number }

interface AppContextType {
  jwtToken: string | null;
  subscriptionStatus: string | null;
  adminPassword: string;
  isLocked: boolean;
  pin: string;
  isShaking: boolean;
  requests: Agent[];
  authorized: Agent[];
  auditLogs: AuditLog[];
  tasks: ScheduledTask[];
  permissions: Permission[];
  inboxFiles: InboxFile[];
  knowledgeDirs: KnowledgeDir[];
  groups: Group[];
  loading: boolean;
  setJwtToken: (token: string | null) => void;
  setSubscriptionStatus: (status: string | null) => void;
  setIsLocked: (locked: boolean) => void;
  setPin: React.Dispatch<React.SetStateAction<string>>;
  setIsShaking: (shaking: boolean) => void;
  loadData: () => Promise<void>;
  correctPin: string;
}

const AppContext = createContext<AppContextType | undefined>(undefined);

export const AppProvider = ({ children }: { children: ReactNode }) => {
  const [jwtToken, setJwtToken] = useState<string | null>(localStorage.getItem('baton_jwt'));
  const [subscriptionStatus, setSubscriptionStatus] = useState<string | null>(localStorage.getItem('baton_subscription_status'));
  const [adminPassword, setAdminPassword] = useState("");
  const [isLocked, setIsLocked] = useState(true);
  const [pin, setPin] = useState("");
  const [isShaking, setIsShaking] = useState(false);
  const [loading, setLoading] = useState(true);

  const [requests, setRequests] = useState<Agent[]>([]);
  const [authorized, setAuthorized] = useState<Agent[]>([]);
  const [auditLogs, setAuditLogs] = useState<AuditLog[]>([]);
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [permissions, setPermissions] = useState<Permission[]>([]);
  const [inboxFiles, setInboxFiles] = useState<InboxFile[]>([]);
  const [knowledgeDirs, setKnowledgeDirs] = useState<KnowledgeDir[]>([]);
  const [groups, setGroups] = useState<Group[]>([]);

  const previousPendingCount = useRef(0);
  const correctPin = localStorage.getItem('baton_app_pin') || "0000";

  useEffect(() => {
    invoke<string>('get_admin_password').then(setAdminPassword).catch(console.error);
  }, []);

  const hasLoadedOnce = useRef(false);

  const loadData = useCallback(async (signal?: AbortSignal) => {
    if (!adminPassword) {
      setLoading(false);
      return;
    }
    if (!hasLoadedOnce.current) {
      setLoading(true);
    }
    const headers = { 'Authorization': basicAuthHeader(adminPassword) };
    try {
      await Promise.allSettled([
        fetch(`${API_BASE}/admin/api/pending`, { signal, headers }).then(async res => { 
          if (res.ok) {
            const data = await res.json();
            if (data.length > previousPendingCount.current) {
              isPermissionGranted().then(granted => {
                if (granted) sendNotification({ title: 'Baton', body: 'New agent requesting access!' });
                else requestPermission().then(p => { if (p === 'granted') sendNotification({ title: 'Baton', body: 'New agent requesting access!' }); });
              });
            }
            previousPendingCount.current = data.length;
            setRequests(data);
          }
        }),
        fetch(`${API_BASE}/admin/api/authorized`, { signal, headers }).then(async res => { if (res.ok) setAuthorized(await res.json()); }),
        fetch(`${API_BASE}/api/audit`, { signal, headers }).then(async res => { if (res.ok) setAuditLogs(await res.json()); }),
        fetch(`${API_BASE}/api/schedule`, { signal, headers }).then(async res => { if (res.ok) setTasks(await res.json()); }),
        fetch(`${API_BASE}/api/permissions`, { signal, headers }).then(async res => { if (res.ok) setPermissions(await res.json()); }),
        fetch(`${API_BASE}/api/inbox`, { signal, headers }).then(async res => { if (res.ok) setInboxFiles(await res.json()); }),
        fetch(`${API_BASE}/api/knowledge`, { signal, headers }).then(async res => { if (res.ok) setKnowledgeDirs(await res.json()); }),
        fetch(`${API_BASE}/admin/api/groups`, { signal, headers }).then(async res => { if (res.ok) setGroups(await res.json()); })
      ]);
    } catch (err: unknown) {
      if (err instanceof Error && err.name !== 'AbortError') {
        console.error(err);
      }
    } finally {
      hasLoadedOnce.current = true;
      setLoading(false);
    }
  }, [adminPassword]);

  useEffect(() => {
    if (!jwtToken || subscriptionStatus !== 'premium' || isLocked) return;
    const initialController = new AbortController();
    loadData(initialController.signal);

    const interval = setInterval(() => {
      loadData();
    }, 5000);

    return () => {
      clearInterval(interval);
      initialController.abort();
    };
  }, [adminPassword, jwtToken, subscriptionStatus, isLocked, loadData]);

  return (
    <AppContext.Provider value={{
      jwtToken, setJwtToken,
      subscriptionStatus, setSubscriptionStatus,
      adminPassword,
      isLocked, setIsLocked,
      pin, setPin,
      isShaking, setIsShaking,
      requests, authorized, auditLogs, tasks, permissions, inboxFiles, knowledgeDirs, groups,
      loading, loadData, correctPin
    }}>
      {children}
    </AppContext.Provider>
  );
};

export const useAppContext = () => {
  const context = useContext(AppContext);
  if (context === undefined) {
    throw new Error('useAppContext must be used within an AppProvider');
  }
  return context;
};
