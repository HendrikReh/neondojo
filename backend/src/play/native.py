"""One isolated GNUbg action. Only structured JSON enters this adapter.

Save before hint: GNUbg's hint creates a pending moverecord that must never
become a played move in the durable SGF. 0-ply hint evaluates every result of
GenerateMoves; unlike stronger hint profiles it also avoids expensive searches.
"""
import json
import base64
import resource
import time
import gnubg

with open('request.json', encoding='utf-8') as f:
    request = json.load(f)

def command(value):
    gnubg.command(value)

for value in ['set display off', 'set confirm default yes', 'set confirm new no',
              'set automatic game off', 'set automatic roll off',
              'set automatic move off', 'set automatic bearoff off',
              'set automatic crawford on', 'set tutor mode off', 'set beavers 0',
              'set player 0 human', 'set player 1 human',
              'set player 0 name White', 'set player 1 name Black']:
    command(value)

human = request['human_player']
plies = request['plies']
action = request['action']
resignation_declined = False
external_bot = request.get('external_bot', False)
match_bots = request.get('match_bots')
# Cubeless money duels: no cube, no Jacoby rule, cubeless bot evaluation.
cubeless = request.get('cubeless', False)
assert type(cubeless) is bool
assert not cubeless or match_bots is not None and request['match_length'] == 0
if cubeless:
    # A global setting: it applies before 'new session' (saved as RU[NoCube])
    # and is restored again for every loaded checkpoint.
    command('set cube use off')
assert type(external_bot) is bool
assert type(human) is int and human in (0, 1)
assert type(plies) is int and 0 <= plies <= 2

if request['new']:
    length = request['match_length']
    assert type(length) is int and 0 <= length <= 25
    assert action == 'opening_roll'
    if cubeless:
        command('set jacoby off')
    elif length == 0:
        command('set jacoby on')
    if match_bots is not None:
        command('set rng file dice.txt')
    command('set display on')
    command('new session' if length == 0 else 'new match %d' % length)
    command('set display off')
else:
    if not globals().get('_reuse_checkpoint', False):
        command('load match input.sgf')
        # The loader deliberately navigates backwards for a one-move match.
        # Advance through all games and records before checking the exact state.
        gnubg.navigate(game=10000)
        gnubg.navigate(next=100000)
        # The SGF loader replays scoring but not NextTurn's post-game Crawford
        # transition. Finish that native transition before checking the checkpoint.
        if gnubg.posinfo()['gamestate'] != 1:
            gnubg.nextturn()
    assert gnubg.gnubgid() == request['native_id'], 'SGF did not restore exact saved state: %s != %s' % (gnubg.gnubgid(), request['native_id'])
    if match_bots is not None:
        command('set rng file dice.txt')
    if action == 'opening_roll':
        command('set display on')
        command('new game')
        command('set display off')

def configure_player(player, depth):
    for kind in ('chequerplay', 'cubedecision'):
        for suffix in ('type evaluation', 'evaluation plies %d' % depth,
                       'evaluation cubeful %s' % ('off' if cubeless else 'on'),
                       'evaluation deterministic on',
                       'evaluation noise 0'):
            command('set player %d %s %s' % (player, kind, suffix))


# Apply the captured bot profile before any ComputerTurn.
if match_bots is None:
    configure_player(1 - human, plies)


# An opening result is always paused with both players human. Only explicit
# continuation activates the bot; its first move consumes the same native dice.
if action != 'opening_roll':
    if not external_bot:
        command('set player %d gnubg' % (1 - human))
    before = gnubg.gnubgid()
    if action == 'inspect':
        assert external_bot
    elif action == 'start_opening_move':
        assert gnubg.posinfo()['dice'][0] and not gnubg.posinfo()['doubled']
        if not external_bot:
            gnubg.nextturn()
    elif action == 'move':
        notation = request['notation']
        assert notation and len(notation) <= 128
        assert all(c in '0123456789/baroff*() ' for c in notation)
        command('move ' + notation)
    elif action == 'resign':
        level = request['resign_level']
        assert type(level) is int and 1 <= level <= 3
        command('resign %d' % level)
        if external_bot:
            accept = request['accept_resignation']
            assert type(accept) is bool
            command('accept' if accept else 'decline')
        if gnubg.posinfo()['gamestate'] == 1:
            assert gnubg.gnubgid() == before, 'Declined resignation changed native state'
            resignation_declined = True
        else:
            # GNUbg's live resignation turn is the recipient, while SGF replay
            # restores the resigner. Canonicalize through native replay so the
            # next action can still require an exact checkpoint ID.
            command('save match resigned.sgf')
            command('set player %d human' % (1 - human))
            command('load match resigned.sgf')
            gnubg.navigate(game=10000)
            gnubg.navigate(next=100000)
            gnubg.nextturn()
    elif action == 'pass':
        command('move')
    else:
        assert action in ('roll', 'double', 'take', 'drop')
        command(action)
    if action not in ('start_opening_move', 'inspect') and not resignation_declined:
        assert gnubg.gnubgid() != before, 'GNUbg rejected action without changing state'

# GNUbg can offer a bot resignation during ComputerTurn. The UI has no human
# response to bot resignation offers;
# decline it natively so the bot finishes its turn before saving. A maximum of
# three increasing resignation levels is possible; bound retries independently
# of the subprocess deadline and fail atomically if the engine does not yield.
for _ in range(3):
    pending = gnubg.posinfo()
    if pending['gamestate'] != 1 or not pending['resigned']:
        break
    assert pending['turn'] == human, 'Unexpected resignation recipient'
    command('decline')
assert gnubg.posinfo()['gamestate'] != 1 or not gnubg.posinfo()['resigned'], 'Bot resignation did not resolve'

def process_cpu_us():
    usage = resource.getrusage(resource.RUSAGE_SELF)
    return round((usage.ru_utime + usage.ru_stime) * 1e6)

bot_costs = [{'cpu_us': 0, 'wall_us': 0} for _ in range(2)]

# A native bot acts only on its own turn. Leave the other player human so GNUbg
# yields at the next decision, including an external engine's cube response.
if match_bots is not None and action != 'opening_roll':
    for player, depth in enumerate(match_bots):
        if depth is None:
            continue
        assert type(depth) is int and 0 <= depth <= 4
        configure_player(player, depth)
    for _ in range(4096):
        state = gnubg.posinfo()
        turn = state['turn']
        if state['gamestate'] != 1 or match_bots[turn] is None:
            break
        before = gnubg.gnubgid()
        command('set player %d gnubg' % turn)
        # Engine duel cost: process CPU time of all GNUbg threads and wall time
        # of this native decision step only, without startup or SGF replay.
        cpu_before, wall_before = process_cpu_us(), time.perf_counter_ns()
        gnubg.nextturn()
        bot_costs[turn]['cpu_us'] += process_cpu_us() - cpu_before
        bot_costs[turn]['wall_us'] += (time.perf_counter_ns() - wall_before) // 1000
        for _ in range(3):
            pending = gnubg.posinfo()
            if pending['gamestate'] != 1 or not pending['resigned']:
                break
            command('decline')
        command('set player %d human' % turn)
        state = gnubg.posinfo()
        assert not state['resigned'] and gnubg.gnubgid() != before, 'Native bot did not complete its decision'
    else:
        raise RuntimeError('Native bot action limit exceeded')

position = gnubg.posinfo()
# cubeinfo() rejects final scores at/above match length. Decode the native
# MatchID fields instead; this reads GNUbg state and never computes scoring.
key = int.from_bytes(base64.b64decode(gnubg.gnubgid().split(':')[1]), 'little')
def bits(offset, width):
    return (key >> offset) & ((1 << width) - 1)
cube = {'cube': 1 << bits(0, 4), 'cubeowner': bits(4, 2),
        'move': bits(6, 1), 'crawford': bits(7, 1),
        'matchto': bits(21, 15), 'score': [bits(36, 15), bits(51, 15)]}
if cube['cubeowner'] == 3:
    cube['cubeowner'] = -1
match = gnubg.match(analysis=0, statistics=0)
games = match['games']
info = games[-1]['info']
winner = {'X': 0, 'O': 1}.get(info.get('winner'))
scores = list(cube['score'])
if action == 'opening_roll':
    phase = 'opening_result'
elif cube['matchto'] > 0 and max(scores) >= cube['matchto']:
    phase = 'match_over'
elif position['gamestate'] != 1:
    phase = 'game_over'
elif position['doubled']:
    phase = 'double_response'
elif position['dice'][0]:
    phase = 'move'
else:
    phase = 'roll'
assert external_bot or phase in ('opening_result', 'game_over', 'match_over') or position['turn'] == human, 'Bot did not yield to human'

# msBoard is relative to cubeinfo.move, which differs from turn while doubled.
board = [0] * 26
native_board = gnubg.board()
for i in range(25):
    white = native_board[1 if cube['move'] == 0 else 0][i]
    black = native_board[0 if cube['move'] == 0 else 1][i]
    board[i + 1] += white
    board[24 - i] -= black

history = []
for game in games:
    for record in game['game']:
        action = record['action']
        if action not in ('move', 'double', 'take', 'drop', 'resign'):
            continue
        player = 0 if record['player'] == 'X' else 1
        moves = record.get('move', [])
        def point(n):
            return 'bar' if n == 25 else 'off' if n == 0 else str(n)
        notation = ' '.join(point(a) + '/' + point(b) for a, b in moves) if action == 'move' else action
        history.append({'player': player, 'dice': list(record.get('dice', [])),
                        'notation': notation or 'pass'})

native_id = gnubg.gnubgid()
command('save match output.sgf')
# GNUbg may export pmr_hint as an extra played action even without hint().
# Remove precisely that transient node using match()'s played action count,
# retaining PL/DI records. Lexing respects escaped SGF property values.
with open('output.sgf', encoding='utf-8') as f:
    sgf = f.read()
starts = []
in_value = False
escaped = False
for i, char in enumerate(sgf):
    if in_value:
        if escaped:
            escaped = False
        elif char == '\\':
            escaped = True
        elif char == ']':
            in_value = False
    elif char == '[':
        in_value = True
    elif char == ';' and sgf[i + 1:i + 3] in ('W[', 'B['):
        starts.append(i)
# GNUbg represents resignation with the game's RE[...] root property,
# rather than a separate W[...] / B[...] move node.
played_nodes = sum(record['notation'] != 'resign' for record in history)
if len(starts) == played_nodes + 1:
    sgf = sgf[:starts[-1]] + ')\n'
elif len(starts) != played_nodes:
    raise RuntimeError('Native SGF action count differs from played history')
with open('output.sgf', 'w', encoding='utf-8') as f:
    f.write(sgf)
legal_moves = []
if phase == 'move':
    command('set evaluation chequerplay evaluation plies 0')
    for hint in gnubg.hint(10000)['hint']:
        notation = hint['move']
        def absolute(n):
            if n == 0:
                return -1 if position['turn'] == 0 else 26
            return n if position['turn'] == 0 else 25 - n
        steps = [{'from': absolute(a), 'to': absolute(b)} for a, b in gnubg.parsemove(notation)]
        legal_moves.append({'notation': notation, 'steps': steps})

result = {'resignation_declined': resignation_declined, 'native_id': native_id, 'board': board, 'player': position['turn'],
          'dice': list(position['dice']) if position['dice'][0] else [],
          'phase': phase, 'legal_moves': legal_moves, 'history': history,
          'scores': scores, 'game_no': len(games), 'winner': winner,
          'can_double': phase == 'roll' and not cubeless and not cube['crawford']
              and cube['cubeowner'] in (-1, position['turn'])
              and (cube['matchto'] == 0 or cube['cube'] < cube['matchto'] - scores[position['turn']])}
if match_bots is not None:
    rolls = [list(record['dice']) for record in games[-1]['game']
             if record['action'] == 'move']
    if phase == 'move':
        rolls.append(list(position['dice']))
    result['match_rolls'] = rolls
    result['bot_costs'] = bot_costs
with open('snapshot.json', 'w', encoding='utf-8') as f:
    json.dump(result, f, allow_nan=False)
