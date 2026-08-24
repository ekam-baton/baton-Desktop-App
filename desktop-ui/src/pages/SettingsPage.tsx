import { NavLink, Outlet, Navigate, useLocation } from 'react-router-dom';
import { ShieldCheck, ShieldAlert, Settings, Box, FileDown } from 'lucide-react';
import { useAppContext } from '../contexts/AppContext';

export const SettingsPage = () => {
  const { requests } = useAppContext();
  const location = useLocation();

  if (location.pathname === '/settings') {
    return <Navigate to="/settings/general" replace />;
  }

  return (
    <div className="settings-container">
      <div className="settings-sidebar">
        <div className="settings-group-title">Device Management</div>
        <NavLink to="/settings/authorized" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <ShieldCheck size={16}/> Active Devices
        </NavLink>
        <NavLink to="/settings/pending" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <ShieldAlert size={16}/> Pending Requests
          {requests.length > 0 && <span className="badge">{requests.length}</span>}
        </NavLink>
        <NavLink to="/settings/inbox" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <Box size={16}/> Inbox Sync
        </NavLink>
        
        <div className="settings-group-title" style={{marginTop: '1.5rem'}}>Configuration</div>
        <NavLink to="/settings/general" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <Settings size={16}/> General
        </NavLink>
        <NavLink to="/settings/knowledge" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <Box size={16}/> Knowledge Base
        </NavLink>
        <NavLink to="/settings/schedule" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <Settings size={16}/> Scheduled Tasks
        </NavLink>
        <NavLink to="/settings/tunnel" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <Settings size={16}/> Network Tunnel
        </NavLink>
        <NavLink to="/settings/handoff" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <FileDown size={16}/> Context Handoff
        </NavLink>
        
        <div className="settings-group-title" style={{marginTop: '1.5rem'}}>Security</div>
        <NavLink to="/settings/permissions" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <ShieldCheck size={16}/> Permissions
        </NavLink>
        <NavLink to="/settings/audit" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <ShieldAlert size={16}/> Audit Log
        </NavLink>
        <NavLink to="/settings/groups" className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}>
          <ShieldCheck size={16}/> Groups
        </NavLink>
      </div>
      <div className="settings-content">
        <Outlet />
      </div>
    </div>
  );
};
