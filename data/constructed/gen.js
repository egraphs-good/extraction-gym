var nodes = {};

const N = 4000;

function shuffle(array) {
    let currentIndex = array.length;
    while (currentIndex != 0) {
        let randomIndex = Math.floor(Math.random() * currentIndex);
        currentIndex--;
        [array[currentIndex], array[randomIndex]] = [
            array[randomIndex], array[currentIndex]
        ];
    }
}

var list = [];
for (var i = 0; i < N; i++) {
    list.push(i);
}
shuffle(list);

for (var i of list) {
    nodes["node1_" + i] = {
        "children": [],
        "op": "consti" + i,
        "cost": i*100,
        "eclass": "class" + i,
    };
    if (i > 0) {
        nodes["node2_" + i] = {
            "children": ["node1_" + (i-1)],
            "op": "nonconsti" + i,
            "cost": 10,
            "eclass": "class" + i,
        };
    }
}

var D = {
    "nodes": nodes,
    "comment": "",
    "root_eclasses": ["class" + (N-1)],
};

console.log(JSON.stringify(D, null, 2));
