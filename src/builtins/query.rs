use crate::{
  builtins::util::{AsDefFunction, FunctionLikeHosted},
  term::{encoding::Labels, Term},
};
use hvmc::{
  host::{DefRef, Host},
  run::{LabSet, Port, Trg, Wire},
  stdlib::HostedDef,
};
use parking_lot::Mutex;
use std::sync::Arc;

pub(crate) fn make_query_def(host: Arc<Mutex<Host>>, labels: Arc<Labels>) -> DefRef {
  struct Query0 {
    host: Arc<Mutex<Host>>,
    labels: Arc<Labels>,
  }
  impl AsDefFunction for Query0 {
    fn call<M: hvmc::run::Mode>(&self, net: &mut hvmc::run::Net<M>, input: Wire, output: Wire) {
      let app_node = net.create_node(hvmc::run::Tag::Ctr, 0);
      let mut buf = String::new();
      let _ = std::io::stdin().read_line(&mut buf);
      // strip trailing newline
      let buf = buf.strip_suffix('\n').unwrap_or(&buf);
      let text = Term::encode_ok(Term::encode_str(buf));
      let mut labs = (*self.labels).clone();
      net.link_wire_port(output, app_node.p2);
      let Ok(text) = crate::term::encoding::encode_term(&text, &mut labs) else {
        net.link_port_port(Port::ERA, app_node.p1);
        net.link_wire_port(input, app_node.p0);
        return;
      };
      self.host.lock().encode_net(net, Trg::port(app_node.p1), &text);
      net.link_wire_port(input, app_node.p0);
    }
  }
  unsafe { HostedDef::new_hosted(LabSet::ALL, FunctionLikeHosted(Query0 { host, labels })) }
}
