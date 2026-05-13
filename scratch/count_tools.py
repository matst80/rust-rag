
import re

with open('src/mcp.rs', 'r') as f:
    content = f.read()

# Find all tool definitions
# Look for #[tool(...) async fn name(...)
tools = re.findall(r'#\[tool\(.*?\)\]\s+async fn (\w+)', content, re.DOTALL)

for i, name in enumerate(tools):
    print(f"{i}: {name}")
