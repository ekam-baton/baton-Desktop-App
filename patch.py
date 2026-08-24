import os
import re
import json

base_dir = r'C:\Users\yrish\.gemini\antigravity\scratch\baton-Desktop-App'

# 1. ChatPage.tsx
chat_page = os.path.join(base_dir, 'desktop-ui', 'src', 'pages', 'ChatPage.tsx')
with open(chat_page, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    'const [currentContact] = useState<{ isVerified?: boolean; publicKey: string } | null>({\n    isVerified: true,\n    publicKey: "server-public-key"\n  });\n  const [pinnedPublicKey] = useState<string>("pinned-public-key-different");\n  \n  const isMitmAttack = currentContact?.isVerified && currentContact.publicKey !== pinnedPublicKey;',
    'const [currentContact] = useState<{ isVerified?: boolean; publicKey: string } | null>(null);\n  const [pinnedPublicKey] = useState<string | null>(null);\n  \n  const isMitmAttack = currentContact?.isVerified && pinnedPublicKey && currentContact.publicKey !== pinnedPublicKey;'
)

content = content.replace('http://localhost:8081', '${API_BASE}')
if 'const API_BASE' not in content:
    content = content.replace(
        "import { useAppContext } from '../contexts/AppContext';",
        "import { useAppContext } from '../contexts/AppContext';\n\nconst API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';"
    )

with open(chat_page, 'w', encoding='utf-8') as f:
    f.write(content)

# 2. Auth.tsx
auth_page = os.path.join(base_dir, 'desktop-ui', 'src', 'components', 'Auth.tsx')
with open(auth_page, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    'const keys = JSON.parse(keysStr);',
    'const keys = JSON.parse(keysStr);\n\n          // Persist the Olm account so we can decrypt incoming messages later\n          const pickledAccount = account.pickle(new Uint8Array(32)); // TODO: Use proper encryption key\n          localStorage.setItem(\'baton_olm_account\', pickledAccount);'
)
content = content.replace('http://localhost:8081', '${API_BASE}')
content = content.replace('http://localhost:8080', '${API_BASE}')

if 'const API_BASE' not in content:
    content = content.replace(
        "import { initOlm, generateOlmAccount, generateOneTimeKeys } from '../utils/crypto';",
        "import { initOlm, generateOlmAccount, generateOneTimeKeys } from '../utils/crypto';\n\nconst API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';"
    )

with open(auth_page, 'w', encoding='utf-8') as f:
    f.write(content)

# 3. GeneralSettings.tsx
gs_page = os.path.join(base_dir, 'desktop-ui', 'src', 'pages', 'settings', 'GeneralSettings.tsx')
with open(gs_page, 'r', encoding='utf-8') as f:
    content = f.read()

content = re.sub(r"const dummyDb = new Uint8Array\(\[1, 2, 3, 4, 5\]\);.*?toast\.error\('Failed to upload backup'\);\n      }", "toast.error('Cloud backup requires database export. Feature coming soon.');\n      return;", content, flags=re.DOTALL)

with open(gs_page, 'w', encoding='utf-8') as f:
    f.write(content)

# 4. features.rs
features = os.path.join(base_dir, 'mcp-connector', 'src', 'handlers', 'features.rs')
with open(features, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace('http::StatusCode,', 'http::{HeaderMap, StatusCode},')
content = content.replace('use crate::state::AppState;', 'use crate::state::AppState;\nuse crate::handlers::admin::check_auth;')

funcs = [
    'pub async fn get_audit_logs',
    'pub async fn get_permissions',
    'pub async fn update_permission',
    'pub async fn get_schedule',
    'pub async fn create_schedule',
    'pub async fn delete_schedule',
    'pub async fn get_inbox',
    'pub async fn create_knowledge',
    'pub async fn get_knowledge',
    'pub async fn handoff_upload',
]

for func in funcs:
    content = content.replace(f'{func}(\n', f'{func}(\n    headers: HeaderMap,\n')
    if func == 'pub async fn handoff_upload':
        content = content.replace(
            'pub async fn handoff_upload(\n    headers: HeaderMap,\n    State(state): State<Arc<AppState>>,\n    mut multipart: Multipart,\n) -> Result<StatusCode, StatusCode> {\n    let mut file_count = 0;',
            'pub async fn handoff_upload(\n    headers: HeaderMap,\n    State(state): State<Arc<AppState>>,\n    mut multipart: Multipart,\n) -> Result<StatusCode, StatusCode> {\n    check_auth(&headers, &state.config.admin_password)?;\n    let mut file_count = 0;'
        )
    else:
        pattern = re.escape(func) + r'\([^)]+\)\s*->\s*[^\{]+\{\n'
        match = re.search(pattern, content)
        if match:
            start, end = match.span()
            content = content[:end] + '    check_auth(&headers, &state.config.admin_password)?;\n' + content[end:]

content = content.replace(
    'let file_name = field.file_name().unwrap_or("unknown").to_string();\n        let data = field.bytes().await.unwrap_or_default();\n        \n        if !data.is_empty() {\n            let file_path = inbox_dir.join(&file_name);',
    'let raw_name = field.file_name().unwrap_or("unknown").to_string();\n        let safe_name = std::path::Path::new(&raw_name).file_name().and_then(|n| n.to_str()).unwrap_or("unknown_file").to_string();\n        let data = field.bytes().await.unwrap_or_default();\n        \n        if !data.is_empty() {\n            let file_path = inbox_dir.join(&safe_name);'
)
content = content.replace(
    'sqlx::query("INSERT INTO inbox_files (file_name, sender) VALUES ($1, \'Desktop Handoff\')")\n            .bind(&file_name)',
    'sqlx::query("INSERT INTO inbox_files (file_name, sender) VALUES ($1, \'Desktop Handoff\')")\n            .bind(&safe_name)'
)

with open(features, 'w', encoding='utf-8') as f:
    f.write(content)

# 5. useWebRTC.ts
webrtc = os.path.join(base_dir, 'desktop-ui', 'src', 'hooks', 'useWebRTC.ts')
with open(webrtc, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    "iceTransportPolicy: 'relay',",
    "iceTransportPolicy: (window as any).__BATON_ICE_SERVERS__ ? 'relay' : 'all',"
)

with open(webrtc, 'w', encoding='utf-8') as f:
    f.write(content)

# 6. main.rs
main_rs = os.path.join(base_dir, 'mcp-connector', 'src', 'main.rs')
with open(main_rs, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    '.route("/safety-number/{device_id}", get(handlers::admin::safety_number_handler))',
    '.route("/admin/api/safety-number/{device_id}", get(handlers::admin::safety_number_handler))'
)
content = content.replace(
    '.route("/api/handoff", post(handlers::features::handoff_upload))',
    '.route("/api/handoff/upload", post(handlers::features::handoff_upload))'
)

with open(main_rs, 'w', encoding='utf-8') as f:
    f.write(content)

# 7. capabilities
cap = os.path.join(base_dir, 'desktop-ui', 'src-tauri', 'capabilities', 'default.json')
with open(cap, 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    '"permissions": [\n    "core:default",\n    "opener:default"\n  ]',
    '"permissions": [\n    "core:default",\n    "opener:default",\n    "notification:default",\n    "autostart:default"\n  ]'
)

with open(cap, 'w', encoding='utf-8') as f:
    f.write(content)

# 8. tauri.conf.json
tauri_conf = os.path.join(base_dir, 'desktop-ui', 'src-tauri', 'tauri.conf.json')
with open(tauri_conf, 'r', encoding='utf-8') as f:
    content = f.read()

data = json.loads(content)
if 'updater' in data.get('plugins', {}):
    del data['plugins']['updater']

with open(tauri_conf, 'w', encoding='utf-8') as f:
    f.write(json.dumps(data, indent=2))

# 9. AppContext.tsx
app_ctx = os.path.join(base_dir, 'desktop-ui', 'src', 'contexts', 'AppContext.tsx')
with open(app_ctx, 'r', encoding='utf-8') as f:
    content = f.read()

if 'const API_BASE' not in content:
    content = content.replace(
        "import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';",
        "import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';\n\nconst API_BASE = import.meta.env.VITE_MCP_URL || 'http://localhost:8081';"
    )
content = content.replace('http://localhost:8081', '${API_BASE}')

with open(app_ctx, 'w', encoding='utf-8') as f:
    f.write(content)

print('Done')
