import { useEffect, useRef, useState } from 'react';

// The "Dumb Pipe" envelope structure expected by the Cloud Router
interface Envelope {
    destination_id: string;
    sender_id: string;
    payload: string; // JSON string containing WebRTC signaling data
}

type SignalingMessage = 
    | { type: 'offer', sdp: RTCSessionDescriptionInit }
    | { type: 'answer', sdp: RTCSessionDescriptionInit }
    | { type: 'ice_candidate', candidate: RTCIceCandidateInit };

export function useWebRTC(localClientId: string, targetPhoneId: string) {
    const [isConnected, setIsConnected] = useState(false);
    const peerConnection = useRef<RTCPeerConnection | null>(null);
    const signalingSocket = useRef<WebSocket | null>(null);
    const remoteAudioRef = useRef<HTMLAudioElement | null>(null);

    // Initialize the Signaling WebSocket connection to the Cloud Router
    useEffect(() => {
        // Connect directly to the new Cloud Router Dumb Pipe
        const ws = new WebSocket(`ws://localhost:3000/relay/${localClientId}`);
        signalingSocket.current = ws;

        ws.onmessage = async (event) => {
            try {
                // The Cloud Router delivers the exact payload string sent by the phone
                const message: SignalingMessage = JSON.parse(event.data);
                handleSignalingMessage(message);
            } catch (err) {
                console.error("Failed to parse signaling message:", err);
            }
        };

        return () => {
            ws.close();
            peerConnection.current?.close();
        };
    }, [localClientId]);

    const sendSignalingMessage = (msg: SignalingMessage) => {
        if (signalingSocket.current?.readyState === WebSocket.OPEN) {
            const envelope: Envelope = {
                destination_id: targetPhoneId,
                sender_id: localClientId,
                payload: JSON.stringify(msg)
            };
            signalingSocket.current.send(JSON.stringify(envelope));
        }
    };

    const initializePeerConnection = () => {
        if (peerConnection.current) return peerConnection.current;

        const pc = new RTCPeerConnection({
            iceServers: [
                { urls: 'stun:stun.l.google.com:19302' } // Free Google STUN server for NAT traversal
            ]
        });

        // Send our ICE candidates to the phone via the Cloud Router
        pc.onicecandidate = (event) => {
            if (event.candidate) {
                sendSignalingMessage({ type: 'ice_candidate', candidate: event.candidate.toJSON() });
            }
        };

        // When the peer connection receives the audio track from the phone, play it!
        pc.ontrack = (event) => {
            if (!remoteAudioRef.current) {
                remoteAudioRef.current = new Audio();
                remoteAudioRef.current.autoplay = true;
            }
            remoteAudioRef.current.srcObject = event.streams[0];
            setIsConnected(true);
        };

        pc.onconnectionstatechange = () => {
            if (pc.connectionState === 'disconnected' || pc.connectionState === 'failed') {
                setIsConnected(false);
            }
        };

        peerConnection.current = pc;
        return pc;
    };

    const handleSignalingMessage = async (msg: SignalingMessage) => {
        const pc = initializePeerConnection();

        if (msg.type === 'offer') {
            await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp));
            const answer = await pc.createAnswer();
            await pc.setLocalDescription(answer);
            sendSignalingMessage({ type: 'answer', sdp: answer });

        } else if (msg.type === 'answer') {
            await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp));

        } else if (msg.type === 'ice_candidate') {
            await pc.addIceCandidate(new RTCIceCandidate(msg.candidate));
        }
    };

    // The Hybrid Pipeline: We can either pipe local STT/TTS via MCP, or stream raw binary buffers to the backend.
    const startVoiceCall = async () => {
        const pc = initializePeerConnection();
        
        // Request microphone access from the Desktop Hub (to speak back to the agent/phone)
        try {
            const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
            stream.getTracks().forEach(track => pc.addTrack(track, stream));
        } catch (err) {
            console.error("Microphone access denied on Desktop Hub", err);
        }

        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);
        sendSignalingMessage({ type: 'offer', sdp: offer });
    };

    return {
        isConnected,
        startVoiceCall
    };
}
