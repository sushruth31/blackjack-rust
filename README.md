# blackjack-rust — a multiplayer blackjack table served over TCP

[![CI](https://github.com/sushruth31/blackjack-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/sushruth31/blackjack-rust/actions/workflows/ci.yml)

Up to seven people connect with `nc`, sit at one shared table, and play real
blackjack: shoe, soft/hard aces, naturals at 3:2, dealer stand rules, bankrolls.
The interesting part is not the card game — it is that the rules are a pure
state machine (commands in, events out) that has never heard of a socket, so
every rule in the game is tested without opening a port.

## Stack

- **Rust 2021**, no framework.
- **tokio 1.22** for the async runtime and TCP — one task per concern, and
  `mpsc` channels rather than `Arc<Mutex<Game>>`, so the rules need no locking.
- **rand 0.8** for the shoe. `Table` is generic over `Rng`, which is what lets
  the integration tests replay 300 deterministic rounds from a seed.
- No serialization crate: the wire format is one line of text, so `nc` or
  `telnet` is a complete client.

## Running it

```sh
cp .env.example .env          # every knob is documented in there
set -a && . ./.env && set +a  # no dotenv crate; the process env is the config
cargo run --release
```

Then, from as many terminals as you like:

```sh
nc localhost 8080
```

Type a name, then `bet 20`, then `hit` / `stand`. Betting opens once
`BLACKJACK_MIN_PLAYERS` seats are filled, and every card except the dealer's
hole card is broadcast to the whole table.

Start it with no environment and it exits 1 with
`blackjack: BLACKJACK_BIND_ADDR is not set (see .env.example)` rather than
binding something arbitrary.

## Architecture

```
 TCP client ─┬─ reader task ── parse_command ──┐
             │                                 │  Request
             └─ writer task ◄── mailbox(64) ◄┐ │
                                             │ ▼
                                    ┌────────┴──────────┐
                                    │   table task      │  sole owner of Table
                                    │  Command ► Event  │  no locks, no Arc
                                    └───────────────────┘
```

| Module            | Responsibility                                                |
| ----------------- | ------------------------------------------------------------- |
| `src/card.rs`     | Ranks, suits, the shoe, and the shuffle                        |
| `src/hand.rs`     | Totals — the only code that knows an ace is 1 or 11            |
| `src/rules.rs`    | Dealer drawing policy, hand comparison, payouts                |
| `src/table.rs`    | Round state machine: seats, bets, turn order, settlement       |
| `src/protocol.rs` | The wire format — the only code that produces a string         |
| `src/config.rs`   | Environment parsing and eager validation                       |
| `src/main.rs`     | tokio transport: accept, read lines, fan events out            |

`table.rs` returns semantic `Event`s (`Dealt`, `Bust`, `Settled`) rather than
prose. `protocol.rs` turns those into lines. That split is why the tests assert
on outcomes instead of on English.

## Design notes

- **Ace scoring is one pass, not a search.** Score every ace at 11, then demote
  one ace at a time while the hand is over 21 — O(cards), no backtracking. It
  works because at most one ace in a live hand can be worth 11: two would add 22
  on their own. The tests pin the cases a naive sum gets wrong — `A A` is 12,
  `A A 9` is soft 21, `A A 10` is a *hard* 12, and `A 6 10` is a hard 17 that
  the H17 rule must not touch.
- **The shuffle was biased and is not any more.** Swapping card `i` with a card
  drawn from the whole deck produces `n^n` equally likely swap sequences spread
  over `n!` permutations, and `n!` does not divide `n^n`, so some orderings come
  up more often. Fisher-Yates draws from `0..=i` instead and is uniform. A test
  asserts the shuffle is a permutation of the multiset and that a seed
  reproduces it exactly.
- **Rules first, transport second.** `Table::apply` is a pure function from
  `(state, Command)` to `Result<Vec<Event>, TableError>`. That is what makes a
  300-round property test cheap: "if any player stood, the dealer's final total
  is at least 17" is one assertion over real deals, not a mocked socket.
- **One task owns the table, and it never awaits a client.** Outbound lines go
  out with `try_send` into a 64-line bounded mailbox; a client that has stopped
  reading loses lines instead of stalling the round for the other six players.
  Inbound commands *do* await, because backpressure on a chatty client is the
  correct behaviour. Each connection is a reader task plus a writer task rather
  than one task with `select!`, because `AsyncBufReadExt::read_line` is not
  cancellation safe and a cancelled read loses the line boundary.
- **A seven-seat table is a `Vec`, not a `HashMap`.** Seat lookup is a linear
  scan over at most seven elements, which beats hashing a `String` key, and it
  gives the round its turn order for free — "next to act" is the first seat that
  is not done.
- **The dealer does not play out a decided round.** If every wagered hand is
  already bust or a natural, the dealer stands pat: a natural is settled by the
  two cards already on the table, so drawing cannot change any result. Chips are
  whole units, so a 3:2 natural on an odd wager rounds down — the same reason
  casinos ask for even bets, and it is asserted rather than left to chance.

Scope, honestly: no splits, doubles, insurance, or surrender, and no
persistence — bankrolls live for the length of a connection.

## Tests

`cargo test` — 68 tests, all green, ~1700 lines of source including them.

- `src/hand.rs` — 13 tests on soft/hard totals: multiple aces, a soft total
  hardening on the next card, three-card 21 not counting as a natural.
- `src/rules.rs` — 13 tests on dealer policy (S17 vs H17, and H17 not applying
  to a hard 17 that contains an ace), on the player-busts-first ordering, and on
  3:2 rounding.
- `src/table.rs` — 17 tests on the round machine: a wager is deducted exactly
  once, acting out of turn is refused, leaving on your own turn does not
  deadlock the table, and 500 consecutive rounds always reach settlement.
- `src/card.rs`, `src/protocol.rs`, `src/config.rs` — 21 tests on shoe
  composition and shuffle fairness, command parsing, and configuration that
  fails loudly with the offending variable named.
- `tests/round.rs` — 4 end-to-end tests, three of them sweeping 300 seeded
  rounds, driving the public API exactly as the server does.

## License

MIT — see [LICENSE](LICENSE).
