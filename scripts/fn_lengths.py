#!/usr/bin/env python3
"""List non-test functions of at least N lines (default 150): `scripts/fn_lengths.py [N]`."""
import re, sys, glob
res=[]
for f in glob.glob('waymaker-*/src/**/*.rs', recursive=True):
    L=open(f).read().split('\n')
    test_start=next((i for i,l in enumerate(L) if l.startswith('#[cfg(test)]')), len(L))
    i=0
    while i < len(L):
        m=re.match(r'^(\s*)(pub(\([^)]*\))?\s+)?(async\s+)?(unsafe\s+)?fn\s+(\w+)', L[i])
        if m and i < test_start:
            # find opening brace then match
            depth=0; started=False; j=i
            while j < len(L):
                line=re.sub(r'"(\\.|[^"\\])*"','""',L[j]); line=re.sub(r"'(\\.|[^'\\])'","''",line); line=line.split('//')[0]
                for ch in line:
                    if ch=='{': depth+=1; started=True
                    elif ch=='}': depth-=1
                if started and depth==0: break
                if not started and line.rstrip().endswith(';'): break
                j+=1
            if started: res.append((j-i+1, f, i+1, m.group(6)))
        i+=1
res.sort(reverse=True)
for n,f,l,name in res:
    if n>=int(sys.argv[1] if len(sys.argv) > 1 else 150): print(f"{n:5d}  {f}:{l}  {name}")
print('functions >=100:', sum(1 for r in res if r[0]>=100), ' >=200:', sum(1 for r in res if r[0]>=200), ' total fns:', len(res))
