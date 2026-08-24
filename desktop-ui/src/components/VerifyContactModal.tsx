import React, { useEffect, useState } from 'react';
import { generateSecurityNumber } from '../utils/crypto';

interface VerifyContactModalProps {
  contact: { client_id: string; device_name: string; publicKey: string; isVerified?: boolean };
  localPublicKey: string;
  onVerify: () => void;
  onClose: () => void;
}

export const VerifyContactModal: React.FC<VerifyContactModalProps> = ({ contact, localPublicKey, onVerify, onClose }) => {
  const [securityNumber, setSecurityNumber] = useState<string>('');

  useEffect(() => {
    generateSecurityNumber(localPublicKey, contact.publicKey).then(setSecurityNumber);
  }, [localPublicKey, contact.publicKey]);

  // Format into groups of 5
  const formattedNumber = securityNumber.match(/.{1,5}/g)?.join(' ') || '';

  return (
    <div className="fixed inset-0 flex items-center justify-center bg-black bg-opacity-50 z-50">
      <div className="bg-white p-6 rounded shadow-lg max-w-sm w-full">
        <h2 className="text-xl font-bold mb-4">Verify Contact</h2>
        <p className="mb-4 text-sm text-gray-600">
          Compare this 60-digit security number with your contact. If they match, your connection is secure.
        </p>
        <div className="bg-gray-100 p-4 rounded mb-6 font-mono text-center text-lg break-words">
          {formattedNumber || 'Generating...'}
        </div>
        <div className="flex justify-end space-x-2">
          <button onClick={onClose} className="px-4 py-2 border rounded">Cancel</button>
          <button onClick={onVerify} className="px-4 py-2 bg-blue-600 text-white rounded">Mark as Verified</button>
        </div>
      </div>
    </div>
  );
};
