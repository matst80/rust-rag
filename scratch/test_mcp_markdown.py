import urllib.request
import urllib.error
import json
import uuid

def mcp_request(method, params):
    url = "http://127.0.0.1:3000/mcp"
    headers = {"Content-Type": "application/json", "Authorization": "Bearer rust-rag-admin-token"}
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    }
    req = urllib.request.Request(url, data=json.dumps(payload).encode('utf-8'), headers=headers, method='POST')
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        print(f"Error {e.code}: {e.read().decode('utf-8')}")
        raise

def test():
    test_id = f"test_{uuid.uuid4().hex[:8]}"
    print(f"Testing with id: {test_id}")
    
    print("\n--- 1. store_entry ---")
    res = mcp_request("store_entry", {
        "id": test_id,
        "text": "Initial test text.",
        "metadata": {"tags": ["test"], "author": "script"},
        "source_id": "test_source"
    })
    print(json.dumps(res, indent=2))
    
    print("\n--- 2. append_to_entry ---")
    res = mcp_request("append_to_entry", {
        "id": test_id,
        "text": "Appended test text."
    })
    print(json.dumps(res, indent=2))
    
    print("\n--- 3. get_entry (Markdown verification) ---")
    res = mcp_request("get_entry", {
        "id": test_id
    })
    print("Result:")
    print(res.get("result", ""))

if __name__ == "__main__":
    test()
