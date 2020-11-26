
 Continuous integration | License
 -----------------------|--------
![Continuous integration](https://github.com/chimera-kube/pod-toleration-policy/workflows/Continuous%20integration/badge.svg) | [![License: Apache 2.0](https://img.shields.io/badge/License-Apache2.0-brightgreen.svg)](https://opensource.org/licenses/Apache-2.0)

This project defines a Chimera Policy written in Rust.

# The goal

Given the following scenario:

> As an operator of a Kubernetes cluster used by multiple tenants,
> I have reserved a set of nodes to a specific tenant,
> hence I want a policy that prevents untrusted users from running their workloads on these reserved nodes.

This scenario can be implemented using the concepts of
[taints and tolerations](https://kubernetes.io/docs/concepts/scheduling-eviction/taint-and-toleration/)
built into Kubernetes:

  1. Reserved nodes are tainted by the cluster operator. That prevents generic
    workloads from being scheduled on them.
  1. Trusted users should put add a "special" `toleration` to the workloads that
    must be scheduled on their reserved nodes.
  1. Untrusted users should not be able to fool the Kubernetes scheduler by
    adding the "special" `toleration` used by the trusted users.

Unfortunately Kubernetes doesn't have any built-in mechanism that can solve
the last point. This is a task for a specially crafted dynamic admissions
controller, like [Chimera](https://github.com/chimera-kube/chimera-admission).

This Chimera Policy ensures only trusted users can schedule workloads that have
the `toleration` required to be running on the reserved nodes.

The policy does that by inspecting `CREATE` and `UPDATE` requests of
`Pod` resources.

# Usage

Let's assume some nodes of the cluster have been tainted in this way by
the cluster operator:

```shell
$ kubectl taint nodes node1 dedicated=tenantA:NoSchedule
$ kubectl taint nodes node2 dedicated=tenantA:NoSchedule
$ # goes on...
```

That means regular workloads, regardless of the tenant, will never be scheduled
to these nodes.

Only workloads defined with the right toleration can be scheduled on them.

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

Also this workload could be scheduled on the reserved nodes:

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

**Note well:** a toleration with the `Exists` operator could be abused by
an evil user to allow his workloads to be scheduled on a dedicated node.

# How the policy works

This policy allows the cluster operator to define a taint to be "protected".
By protecting a taint nobody in the cluster will be able to tolerate it via the
`Exists` operator.

Only the trusted users will be allowed to create Pods tolerating the protected
taint with a toleration that uses the `Equal` operator.

# Configuration

The policy has no hard-coded value neither for the `taint` nor for the
`usernames` or `groups` that are entitled to tolerate the `taint`.

The code will read these settings from the environment variables:

  * `TAINT_KEY`: `key` of the taint. Required.
  * `TAINT_VALUE`: `value` of the taint. Required.
  * `ALLOWED_USERS`: comma separated list of users who are allowed to use
    this toleration. Optional.
  * `ALLOWED_GROUPS`: comma separated list of groups who are allowed to use
    this toleration. Optional.

# Example

Given a cluster with:

  * Two groups of users: `tenantA-users` and `tenantB-users`
  * Some nodes tainted with the taint `dedicated:tenantA`

And a policy instantiated with this configuration:

  * `TAINT_KEY` = `dedicated`
  * `TAINT_VALUE` = `tenantA`
  * `ALLOWED_GROUPS` = `tenantA-users`

A Pod using the following toleration:

```yaml
  tolerations:
  - key: "dedicated"
    operator: "Exists"
    effect: "NoSchedule"
```

Will always be rejected by the policy, regardless of the group to which the user
belongs to.

On the other hand, a Pod defined with this toleration:

```yaml
  tolerations:
  - key: "dedicated"
    operator: "Equal"
    value: "tenantA"
    effect: "NoSchedule"
```

Will be accepted only when created by a user who belongs to the group `tenantA-users`.

Finally, a Pod with the following toleration:

```yaml
  tolerations:
  - key: "dedicated"
    operator: "Equal"
    value: "experiments"
    effect: "NoSchedule"
```

Would never be rejected by the policy.

# Building

Handle the rust installation using rustup. Then add the `wasm32-wasi` target:

```shell
$ rustup target add wasm32-wasi
```

Use this command to build the Wasm code:

```
$ make build
```

This will produce a `.wasm` file under `target/wasm32-wasi/release/`.

# Trying the policy

The policy is a stand-alone Wasm module, you can invoke it in this way:

```shell
$ cat test_data/req_pod_with_equal_toleration.json | wasmtime run \
              --env TAINT_KEY="dedicated" \
              --env TAINT_VALUE="tenantA" \
              --env ALLOWED_GROUPS="administrators" \
              target/wasm32-wasi/release/pod-toleration-policy.wasm
```

This will produce the following output:

```shell
{"accepted":false,"message":"User not allowed to create Pods that tolerate the taint key: dedicated, value : tenantA"}
```

You can find more example files under the `test_data` directory.

# Benchmark

The following command can be used to benchmark the Wasm module:

```
$ make bench
```

The benchmarks execute the Wasm module via
[wasmtime](https://github.com/bytecodealliance/wasmtime).
The execution times are measured by [hyperfine](https://github.com/sharkdp/hyperfine).
