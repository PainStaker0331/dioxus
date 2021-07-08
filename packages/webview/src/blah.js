class OPTABLE {
  PushRoot(self, edit) {
    const id = edit.root;
    const node = self.nodes[id];
    self.stack.push(node);
  }
  AppendChild(self, edit) {
    // todo: prevent merging of text nodes
    const node = self.pop();
    self.top().appendChild(node);
  }
  ReplaceWith(self, edit) {
    const newNode = self.pop();
    const oldNode = self.pop();
    oldNode.replaceWith(newNode);
    self.stack.push(newNode);
  }
  Remove(self, edit) {
    const node = self.stack.pop();
    node.remove();
  }
  RemoveAllChildren(self, edit) {
    // todo - we never actually call this one
  }
  CreateTextNode(self, edit) {
    self.stack.push(document.createTextNode(edit.text));
  }
  CreateElement(self, edit) {
    const tagName = edit.tag;
    console.log(`creating element! ${edit}`);
    self.stack.push(document.createElement(tagName));
  }
  CreateElementNs(self, edit) {
    self.stack.push(document.createElementNS(edit.ns, edit.tag));
  }
  CreatePlaceholder(self, edit) {
    self.stack.push(document.createElement("pre"));
  }
  NewEventListener(self, edit) {
    // todo
  }
  RemoveEventListener(self, edit) {
    // todo
  }
  SetText(self, edit) {
    self.top().textContent = edit.text;
  }
  SetAttribute(self, edit) {
    const name = edit.field;
    const value = edit.value;
    const node = self.top(self.stack);
    node.setAttribute(name, value);

    // Some attributes are "volatile" and don't work through `setAttribute`.
    if ((name === "value", self)) {
      node.value = value;
    }
    if ((name === "checked", self)) {
      node.checked = true;
    }
    if ((name === "selected", self)) {
      node.selected = true;
    }
  }
  RemoveAttribute(self, edit) {
    const name = edit.field;
    const node = self.top(self.stack);
    node.removeAttribute(name);

    // Some attributes are "volatile" and don't work through `removeAttribute`.
    if ((name === "value", self)) {
      node.value = null;
    }
    if ((name === "checked", self)) {
      node.checked = false;
    }
    if ((name === "selected", self)) {
      node.selected = false;
    }
  }
}
