import { Routes, Route, Navigate } from "react-router-dom";
import "./App.css";

import { AuthScreen } from "./components/Auth";
import { MainLayout } from "./components/Layout/MainLayout";
import { useAppContext } from "./contexts/AppContext";

import { LockScreenPage } from "./pages/LockScreenPage";
import { PremiumPage } from "./pages/PremiumPage";
import { ChatPage } from "./pages/ChatPage";
import { SettingsPage } from "./pages/SettingsPage";
import { AdaptersPage } from "./pages/AdaptersPage";

import { GeneralSettings } from "./pages/settings/GeneralSettings";
import { PermissionsSettings } from "./pages/settings/PermissionsSettings";
import { AuditLogSettings } from "./pages/settings/AuditLogSettings";
import { PendingRequests } from "./pages/settings/PendingRequests";
import { AuthorizedAgents } from "./pages/settings/AuthorizedAgents";
import { InboxSettings } from "./pages/settings/InboxSettings";
import { KnowledgeBaseSettings } from "./pages/settings/KnowledgeBaseSettings";
import { ScheduleSettings } from "./pages/settings/ScheduleSettings";
import { TunnelSettings } from "./pages/settings/TunnelSettings";
import { HandoffSettings } from "./pages/settings/HandoffSettings";
import { GroupsSettings } from "./pages/settings/GroupsSettings";

function App() {
  const { jwtToken, subscriptionStatus, isLocked, setJwtToken, setSubscriptionStatus } = useAppContext();

  if (!jwtToken) {
    return (
      <AuthScreen 
        onAuthenticated={(token, refreshToken, subStatus) => {
          localStorage.setItem('baton_jwt', token);
          localStorage.setItem('baton_refresh_token', refreshToken);
          localStorage.setItem('baton_subscription_status', subStatus || 'free');
          setJwtToken(token);
          setSubscriptionStatus(subStatus || 'free');
        }} 
      />
    );
  }

  if (subscriptionStatus !== 'premium') {
    return <PremiumPage />;
  }

  if (isLocked) {
    return <LockScreenPage />;
  }

  return (
    <Routes>
      <Route path="/" element={<MainLayout />}>
        <Route index element={<Navigate to="/chat" replace />} />
        <Route path="chat" element={<ChatPage />} />
        <Route path="adapters" element={<AdaptersPage />} />
        
        <Route path="settings" element={<SettingsPage />}>
          <Route path="authorized" element={<AuthorizedAgents />} />
          <Route path="pending" element={<PendingRequests />} />
          <Route path="inbox" element={<InboxSettings />} />
          <Route path="general" element={<GeneralSettings />} />
          <Route path="knowledge" element={<KnowledgeBaseSettings />} />
          <Route path="schedule" element={<ScheduleSettings />} />
          <Route path="tunnel" element={<TunnelSettings />} />
          <Route path="handoff" element={<HandoffSettings />} />
          <Route path="permissions" element={<PermissionsSettings />} />
          <Route path="audit" element={<AuditLogSettings />} />
          <Route path="groups" element={<GroupsSettings />} />
        </Route>
        
        <Route path="*" element={<Navigate to="/chat" replace />} />
      </Route>
    </Routes>
  );
}

export default App;
