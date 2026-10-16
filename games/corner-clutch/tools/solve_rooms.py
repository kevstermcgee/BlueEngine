#!/usr/bin/env python3
"""Independent breadth-first check of the authored puzzle rooms. No engine imports.

The Rust tests replay the supplied solutions through public Intent; this search
checks the board rules independently and reports minimum solution lengths.
"""
from collections import deque
from pathlib import Path
import json

def neighbors(state, walls, fixed=None, max_crew=None):
    for driver, origin in enumerate(state):
        crew = {driver}
        for j, cell in enumerate(state):
            if abs(cell % 6-origin % 6) == 1 and abs(cell // 6-origin // 6) == 1:
                crew.add(j)
        if max_crew is not None and len(crew) > max_crew:
            continue
        for dx, dy in ((0,-1),(-1,0),(0,1),(1,0)):
            result = list(state)
            for j in crew:
                x, y = state[j] % 6 + dx, state[j] // 6 + dy
                if not (0 <= x < 6 and 0 <= y < 6):
                    break
                cell = y * 6 + x
                if cell in walls or any(k not in crew and c == cell for k,c in enumerate(state)):
                    break
                result[j] = cell
            else:
                if fixed is None or result[fixed] == state[fixed]:
                    yield tuple(result)

def distance(start, goal, walls, fixed=None, max_crew=None):
    queue=deque([(start,0)])
    seen={start}
    while queue:
        state, steps = queue.popleft()
        if state == goal:
            return steps, len(seen)
        for result in neighbors(state,walls,fixed,max_crew):
            if result not in seen:
                seen.add(result)
                queue.append((result,steps+1))
    return None,len(seen)

def main():
    rooms=json.loads((Path(__file__).resolve().parents[1]/'assets/rooms.json').read_text())
    for n, room in enumerate(rooms):
        pack=lambda cs:tuple(y*6+x for x,y in cs)
        start,goal=pack(room['start']),pack(room['goals'])
        walls=set(pack(room['walls']))
        steps,states=distance(start,goal,walls)
        assert steps is not None, room['title']
        assert steps==len(room['solution']), (room['title'],steps)
        print(f"{n+1}. {room['title']}: shortest {steps} moves; {states} states searched",flush=True)
        if n==2:
            without,visited=distance(start,goal,walls,max_crew=2)
            assert without is None or without>steps, 'Middle driver is unnecessary in an optimal solution'
            print(f'   Without a three-crate carry: {without} moves ({visited} states checked).',flush=True)
        if n==4:
            fixed=next(j for j in range(len(start)) if start[j]==goal[j])
            without,visited=distance(start,goal,walls,fixed)
            assert without is None, 'Parked helper can be left untouched'
            print(f'   Parked helper must leave its socket ({visited} states exhaustively checked).',flush=True)
if __name__=='__main__':
    main()
