# Test-only stand-in for `codepet rpc` used by qml/tests/quickshell/shell.qml.
# Reads one request line from stdin and answers per mode. Never used by the product.
import json, sys, time
mode = sys.argv[1]
line = sys.stdin.readline()
req = json.loads(line)
rid = req["id"]
def out(obj):
    sys.stdout.write(json.dumps(obj) + "\n"); sys.stdout.flush()
if mode == "ok":
    m = req["method"]
    if m == "state.get":
        out({"v":1,"id":rid,"ok":True,"result":{"state_version":2,"meters":{"hunger":7000,"thirst":7000,"mental_health":9000,"hygiene":7000},"echo_params":req["params"]}})
    elif m == "fail":
        out({"v":1,"id":rid,"ok":False,"error":{"code":"BUSY","message":"slot taken","retryable":False}})
    else:
        out({"v":1,"id":rid,"ok":True,"result":{}})
elif mode == "noresp":
    sys.stderr.write("stub: refusing\n"); sys.exit(1)
elif mode == "garbage":
    out("x"); sys.stdout.write("not json\n")
elif mode == "wrongid":
    out({"v":1,"id":"other","ok":True,"result":{}})
elif mode == "hang":
    time.sleep(30)
