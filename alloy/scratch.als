module Scratch

sig Id {
    next: lone Id
}

one sig firstId, lastId in Id {}


fact {
    no lastId.next
    all i: Id | i in firstId.*next
    all disj n, m: Node | n.id != m.id
}

sig Node {
    next: one Node,
    id: one Id,
    var inbox: set Id,
    var outbox: set Id,
}

var sig Elected in Node {}

fact allConnected {
    all n, m: Node | m in n.*next
}

fact {
    no inbox and no outbox
    no Elected
}

pred initiate[n: Node] {
    historically not n.id in n.outbox
    
    n.outbox' = n.outbox + n.id

    all other: Node - n | other.outbox' = other.outbox
    inbox' = inbox
    Elected' = Elected
}

fun maxId[s: set Id] : lone Id {
    { i: s | no (s - i) & i.^next }
}

pred send[n: Node] {
    some n.outbox

    n.next.inbox' = n.next.inbox + maxId[n.outbox]
    n.outbox' = none

    all m: Node - n | m.outbox' = m.outbox
    all m: Node - n.next | m.inbox' = m.inbox
    Elected' = Elected
}

pred process[n: Node] {
    some n.inbox
    let maxReceivedId = maxId[n.inbox] {
        -- own id came back around: elect self
        maxReceivedId = n.id implies {
            Elected' = Elected + n
            n.outbox' = none
        }
        -- incoming id is higher: forward it
        else maxReceivedId in n.id.^next implies {
            Elected' = Elected
            n.outbox' = n.outbox + maxReceivedId
        }
        -- incoming id is lower: drop it
        else {
            Elected' = Elected
            n.outbox' = none
        }
    }
    n.inbox' = none
    all m: Node - n | m.inbox' = m.inbox
    all m: Node - n | m.outbox' = m.outbox
}

pred stutter {
    outbox' = outbox
    inbox' = inbox
    Elected' = Elected
}

fact {
    always ((some n: Node | initiate[n]) or 
    (some n: Node | send[n]) or 
    (some n: Node | process[n]) or 
    stutter)
}

// ── Event reification: makes events visible in the visualizer ──

enum Event { Initiate, Stutter, Send, Process }

fun _initiate : Event -> Node {
    Initiate -> { n : Node | initiate[n] }
}

fun _send : Event -> Node {
    Send -> { n : Node | send[n] }
}

fun _process : Event -> Node {
    Process -> { n : Node | process[n] }
}

fun _stutter : set Event {
    { e : Stutter | stutter }
}

fun events : set Event {
    _initiate.Node + _stutter
}

assert atMostOneLeader {
    always lone Elected
}

check atMostOneLeader for 1.. steps

run example { eventually some Elected } for exactly 3 Node, exactly 3 Id, 20 steps
