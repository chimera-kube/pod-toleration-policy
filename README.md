
 Continuous integration | License
 -----------------------|--------
![Continuous integration](https://github.com/chimera-kube/pod-toleration-policy/workflows/Continuous%20integration/badge.svg) | [![License: Apache 2.0](https://img.shields.io/badge/License-Apache2.0-brightgreen.svg)](https://opensource.org/licenses/Apache-2.0)

This project containers a Chimera Policy written using Rust.

# The goal

Given the following scenario:

> As an operator of a Kubernetes cluster used by multiple tenants,
> I have reserved a set of nodes to a specific tenant,
> hence I want a policy that prevents untrusted users from running their workloads on these reserved nodes.

This scenario can be implemented using the concepts of
[taints and tolerations](https://kubernetes.io/docs/concepts/scheduling-eviction/taint-and-toleration/)
built into Kubernetes:

  1. Reserved nodes are tainted by the cluster operator. That prevents generic
    workloads from being scheduled on these nodes.
  1. Trusted users should put add a special `toleration` to the workloads that
    must be scheduled on their reserved nodes.
  1. Untrusted users should not be able to fool the Kubernetes scheduler by
    adding the special `toleration` used by the trusted users.

Unfortunately Kubernetes doesn't have any built-in mechanism that can solve
the last point. This is a task for a specially crafted dynamic admissions
controller, like [Chimera](https://github.com/chimera-kube/chimera-admission).

This Chimera Policy ensures only trusted users can schedule workloads that have
the `toleration` required to be running on the reserved nodes.
The policy does that by inspecting `CREATE` and `UPDATE` requests of
`Pod` resources.

## Examples

Let's assume some nodes of the cluster have been tainted in this way by
the cluster operator:

```shell
$ kubectl taint nodes node1 dedicated=tenantA:NoSchedule
$ kubectl taint nodes node2 dedicated=tenantA:NoSchedule
$ # goes on...
```

That means regular workloads, regardless of the tenant, will never be scheduled
to these nodes.

Only workloads defined with the right toleration will be schedulable on them.
For example, this workload could be scheduled on one of these nodes:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: nginx
spec:
  containers:
  - name: nginx
    image: nginx
    imagePullPolicy: IfNotPresent
  tolerations:
  - key: "dedicated"
    operator: "Equal"
    value: "tenantA"
    effect: "NoSchedule"
```

This workload would be schedulable on the reserved nodes as well:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: nginx
spec:
  containers:
  - name: nginx
    image: nginx
    imagePullPolicy: IfNotPresent
  tolerations:
  - key: "dedicated"
    operator: "Exists"
    effect: "NoSchedule"
```

This policy ensures only trusted users or users who belong to a trusted group
can write workloads with the `tolerations` specified above.

# Configuration

The policy has no hard-coded value for neither the `toleration` nor the
`usernames` or `groups` that are entitled to use the toleration.

The code will read these settings from the environment variables:

  * `TOLERATION_KEY`: `key` of the toleration. Required.
  * `TOLERATION_OPERATOR`: `operator` of the toleration. Required.
  * `TOLERATION_EFFECT`: `effect` of the toleration. Required.
  * `ALLOWED_USERS`: comma separated list of users who are allowed to use
    this toleration. Optional.
  * `ALLOWED_GROUPS`: comma separated list of groups who are allowed to use
    this toleration. Optional.

# Requirements

Handle the rust installation using rustup.

```bash
$ rustup target add wasm32-wasi
```

The snippet above adds a new target called `wasm32-wasi`.

# Building

Use this command to build the WASM code:

```
$ make build
```

This will produce a `.wasm` file under `target/wasm32-wasi/release/`.

# Trying the policy

The policy is a stand-alone WASM module, you can invoke it in this way:

```bash
$ cat test_data/req_pod_with_toleration.json | wasmtime run --env TOLERATION_EFFECT="NoSchedule" \
               --env TOLERATION_KEY="example-key" \
               --env TOLERATION_OPERATOR="Exists" \
               --env ALLOWED_GROUPS="administrators" \
               target/wasm32-wasi/release/real-policy-rust.wasm
```

This will produce the following output:

```bash
{"accepted":false,"message":"User not allowed to create Pod objects with toleration: key: example-key, operator: Exists, effect: NoSchedule)"}
```

You can find more example files under the `test_data` directory.

# Benchmark

The following command can be used to benchmark the WASM module:

```
$ make bench
```

The benchmarks execute the WASM module via
[wasmtime](https://github.com/bytecodealliance/wasmtime).
The execution times are measured by [hyperfine](https://github.com/sharkdp/hyperfine).
