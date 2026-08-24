import { useEffect } from 'react';
import logo from '../assets/logo.png';
import { useAppContext } from '../contexts/AppContext';

export const LockScreenPage = () => {
  const { pin, setPin, isShaking, setIsShaking, correctPin, setIsLocked } = useAppContext();

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key >= '0' && e.key <= '9') {
        if (pin.length < 4) {
          const newPin = pin + e.key;
          setPin(newPin);
          if (newPin === correctPin) {
            setIsLocked(false);
          } else if (newPin.length === 4) {
            setIsShaking(true);
            setTimeout(() => setIsShaking(false), 300);
            setTimeout(() => setPin(""), 350);
          }
        }
      } else if (e.key === 'Backspace' || e.key === 'Delete') {
        setPin(pin.slice(0, -1));
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [pin, correctPin, setPin, setIsLocked, setIsShaking]);

  const handleKeypadPress = (num: string) => {
    if (pin.length < 4) {
      const newPin = pin + num;
      setPin(newPin);
      if (newPin === correctPin) setIsLocked(false);
      else if (newPin.length === 4) { 
        setIsShaking(true); 
        setTimeout(() => setIsShaking(false), 300); 
        setTimeout(() => setPin(""), 350); 
      }
    }
  };

  return (
    <div className="layout" style={{ justifyContent: 'center', alignItems: 'center' }}>
      <div className="lock-screen">
        <img src={logo} alt="Baton Logo" style={{ width: '60px', marginBottom: '1.5rem', filter: 'grayscale(100%) contrast(1.2)' }} />
        <h2 style={{ marginBottom: '2rem' }}>Authentication Required</h2>
        <div style={{ display: 'flex', gap: '1rem', justifyContent: 'center', marginBottom: '2.5rem' }}>
          {[0, 1, 2, 3].map(i => (
            <input
              key={i}
              type="password"
              maxLength={1}
              value={pin[i] || ""}
              readOnly
              className={`lock-input ${isShaking ? 'shake error' : ''}`}
            />
          ))}
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '0.5rem', maxWidth: '280px', margin: '0 auto' }}>
          {[1, 2, 3, 4, 5, 6, 7, 8, 9].map(num => (
            <button 
              key={num}
              className="keypad-btn"
              onClick={() => handleKeypadPress(num.toString())}
            >
              {num}
            </button>
          ))}
          <div></div>
          <button 
            className="keypad-btn"
            onClick={() => handleKeypadPress("0")}
          >
            0
          </button>
          <button 
            className="keypad-btn keypad-btn-del"
            onClick={() => setPin(pin.slice(0, -1))}
          >
            Del
          </button>
        </div>
      </div>
    </div>
  );
};
