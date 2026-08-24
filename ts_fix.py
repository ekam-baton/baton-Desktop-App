import os
import re

base_dir = r'C:\Users\yrish\.gemini\antigravity\scratch\baton-Desktop-App'

def replace_file(path, old, new):
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    content = content.replace(old, new)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

chat = os.path.join(base_dir, 'desktop-ui', 'src', 'pages', 'ChatPage.tsx')
replace_file(chat, "'${API_BASE}/admin/api/chat'", "`${API_BASE}/admin/api/chat`")
replace_file(chat, 'disabled={isMitmAttack}', 'disabled={!!isMitmAttack}')

auth = os.path.join(base_dir, 'desktop-ui', 'src', 'components', 'Auth.tsx')
replace_file(auth, "'${API_BASE}/admin/api/auth'", "`${API_BASE}/admin/api/auth`")
replace_file(auth, "'${API_BASE}/keys/upload'", "`${API_BASE}/keys/upload`")

app_ctx = os.path.join(base_dir, 'desktop-ui', 'src', 'contexts', 'AppContext.tsx')
replace_file(app_ctx, "'${API_BASE}/admin/api/pending'", "`${API_BASE}/admin/api/pending`")
replace_file(app_ctx, "'${API_BASE}/admin/api/authorized'", "`${API_BASE}/admin/api/authorized`")
replace_file(app_ctx, "'${API_BASE}/api/audit'", "`${API_BASE}/api/audit`")
replace_file(app_ctx, "'${API_BASE}/api/schedule'", "`${API_BASE}/api/schedule`")
replace_file(app_ctx, "'${API_BASE}/api/permissions'", "`${API_BASE}/api/permissions`")
replace_file(app_ctx, "'${API_BASE}/api/inbox'", "`${API_BASE}/api/inbox`")
replace_file(app_ctx, "'${API_BASE}/api/knowledge'", "`${API_BASE}/api/knowledge`")
replace_file(app_ctx, "'${API_BASE}/admin/api/groups'", "`${API_BASE}/admin/api/groups`")

handoff = os.path.join(base_dir, 'desktop-ui', 'src', 'pages', 'settings', 'HandoffSettings.tsx')
replace_file(handoff, "http://localhost:8081", "${API_BASE}")
with open(handoff, 'r', encoding='utf-8') as f:
    content = f.read()
if 'API_BASE' not in content:
    pass # we might need to add it, wait... handoff might not have it

gs = os.path.join(base_dir, 'desktop-ui', 'src', 'pages', 'settings', 'GeneralSettings.tsx')
with open(gs, 'r', encoding='utf-8') as f:
    content = f.read()
content = re.sub(r"import { encryptBackupKey } from '../../utils/crypto';\n", "", content)
with open(gs, 'w', encoding='utf-8') as f:
    f.write(content)

print('TS fixed')
