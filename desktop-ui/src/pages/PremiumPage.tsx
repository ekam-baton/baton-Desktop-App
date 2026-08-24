import logo from '../assets/logo.png';
import { useAppContext } from '../contexts/AppContext';

export const PremiumPage = () => {
  const { setJwtToken, setSubscriptionStatus } = useAppContext();

  return (
    <div className="layout" style={{ justifyContent: 'center', alignItems: 'center' }}>
      <div className="lock-screen" style={{ width: '450px', padding: '3rem', textAlign: 'center' }}>
        <img src={logo} alt="Baton Logo" style={{ width: '60px', marginBottom: '1.5rem', filter: 'grayscale(100%) contrast(1.2)' }} />
        <h2 style={{ marginBottom: '1rem' }}>Premium Required</h2>
        <p className="subtitle" style={{ marginBottom: '2rem' }}>
          To unlock Baton Desktop, please subscribe to Premium via the Baton app on your Android or iOS device. 
          Your desktop app will automatically unlock once verified.
        </p>
        <button 
          className="btn-approve" 
          onClick={() => {
            localStorage.removeItem('baton_jwt');
            localStorage.removeItem('baton_subscription_status');
            setJwtToken(null);
            setSubscriptionStatus(null);
          }}
        >
          Log Out
        </button>
      </div>
    </div>
  );
};
