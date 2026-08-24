import { NavLink } from 'react-router-dom';
import { MessageSquare, Settings, Server } from 'lucide-react';
import logo from '../../assets/logo.png';
import { useAppContext } from '../../contexts/AppContext';

export const Sidebar = () => {
  const { requests } = useAppContext();

  return (
    <aside className="sidebar">
      <div className="sidebar-logo">
        <img src={logo} alt="Baton Logo" className="logo-image" />
        <h2>Baton</h2>
      </div>
      <nav className="sidebar-nav">
        <NavLink 
          to="/chat" 
          className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}
        >
          <MessageSquare size={18} /> AI Chat
        </NavLink>
        <NavLink 
          to="/adapters" 
          className={({ isActive }) => `nav-item ${isActive ? 'active' : ''}`}
        >
          <Server size={18} /> Adapters
        </NavLink>
        <NavLink 
          to="/settings" 
          className={({ isActive }) => `nav-item ${isActive || window.location.pathname.startsWith('/settings') ? 'active' : ''}`}
        >
          <Settings size={18} /> Settings
          {requests.length > 0 && <span className="badge" style={{ backgroundColor: 'var(--danger)' }}>{requests.length}</span>}
        </NavLink>
      </nav>
    </aside>
  );
};
