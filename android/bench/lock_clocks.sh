#!/bin/bash
# Locks a rooted device's CPU and GPU clocks for benchmarking, and restores them (docs/12,
# "Fixed clocks"; D640). Host-side: it runs `adb shell` (as root: `adb root` first, on a
# userdebug build).
#
#   lock_clocks.sh lock STATE [POLICY=KHZ ...] [gpu=KHZ] [mif=KHZ] [placement=UCLAMP]
#   lock_clocks.sh verify STATE      the locked clocks still hold (exit 1 if not)
#   lock_clocks.sh restore STATE     writes the recorded originals back and checks them
#   lock_clocks.sh status            each policy's and device's limits and current clock
#
# `lock` records each limit's original value in STATE (a host file) before it writes anything,
# then sets each CPU policy's scaling_min_freq and scaling_max_freq to one frequency, and the
# GPU's (Mali: /sys/devices/platform/*.mali/scaling_{min,max}_freq) when one is found. With an
# existing STATE it keeps that record (the device is already locked: its originals are there)
# and only writes the new frequencies. `restore` writes back the originals of what any lock
# wrote, checks them and removes STATE. Nothing else on the device is changed: no service is
# stopped, no property set.
#
# The defaults are the Pixel 8 Pro's (Tensor G3), about the middle of each range (D640): little
# 1,197 MHz (of 324 to 1,704), mid 1,328 MHz (402 to 2,367), big 1,745 MHz (500 to 2,914), the
# GPU 521 MHz (150 to 890). They held 10 minutes of the stress animation, with and without
# `placement=512`, at thermal status 0; the power HAL's FIXED_PERFORMANCE values (1,425, 1,572,
# 1,745 MHz) didn't: the thermal HAL capped the mid cluster after 576 s. `policy0=1197000` (say) overrides one; `gpu=none` leaves the GPU alone; `mif=KHZ`
# writes the memory interconnect's devfreq too (Exynos's devfreq_mif: min_freq and max_freq),
# but on a Pixel it doesn't hold: the power HAL writes the MIF's, DSU's and BCI's minimum at every
# app launch and CPU load reset (an ADPF hint), then resets it to its own default.
#
# `placement=UCLAMP` (0 to 1024; not set by default) also raises the top-app group's minimum
# utilization clamp, so the foreground app's threads run on the mid and big cores whatever their
# load or hint-session boost (D641). On this Pixel the scheduler's groups are the vendor
# scheduler's (/proc/vendor_sched/groups/ta/uclamp_min): the cpu cgroup's
# /dev/cpuctl/top-app/cpu.uclamp.min holds no task on Android 17 (every task is in the root cpu
# cgroup), so writing it changes nothing.
#
# What it doesn't hold: the scheduler still places threads on the little, mid or big cores (by
# utilization and uclamp, which the hint sessions set), the thermal HAL can still cap a clock
# (its own frequency requests; `verify` reads the current clock to catch it), and the power HAL
# writes these files for some hints (camera, face unlock, low-power modes; not for app launches,
# whose maximum is already the default). measure.py --locked-clocks checks them around each run.
set -euo pipefail

usage() {
    sed -n '2,12p' "$0" >&2
    exit 2
}

[ $# -ge 1 ] || usage
command=$1
shift

CPUFREQ=/sys/devices/system/cpu/cpufreq
DEFAULTS="policy0=1197000 policy4=1328000 policy8=1745000 gpu=521000"

on_device() {
    adb shell "$1" < /dev/null | tr -d '\r'
}

require_root() {
    if [ "$(on_device 'id -u')" != "0" ]; then
        echo "lock_clocks.sh: adbd isn't root: run 'adb root' (a userdebug or eng build)" >&2
        exit 1
    fi
}

gpu_dir() {
    on_device 'for d in /sys/devices/platform/*.mali; do [ -e $d/scaling_max_freq ] && echo $d && break; done; true'
}

mif_dir() {
    on_device 'for d in /sys/class/devfreq/*devfreq_mif; do [ -e $d/min_freq ] && echo $d && break; done; true'
}

# The directory and the file names of a limit's minimum and maximum, by name.
target() {
    case $1 in
        policy*) echo "$CPUFREQ/$1 scaling_min_freq scaling_max_freq" ;;
        gpu) echo "$(gpu_dir) scaling_min_freq scaling_max_freq" ;;
        mif) echo "$(mif_dir) min_freq max_freq" ;;
        # One file, written as both "minimum" and "maximum".
        placement) on_device '[ -e /proc/vendor_sched/groups/ta/uclamp_min ] && echo /proc/vendor_sched/groups/ta uclamp_min uclamp_min; true' ;;
        *) echo "lock_clocks.sh: unknown target $1" >&2; exit 2 ;;
    esac
}

available() {
    local dir=$1 name=$2
    case $name in
        policy*) on_device "cat $dir/scaling_available_frequencies" ;;
        placement) seq 0 1024 | tr '\n' ' ' ;;
        *) on_device "cat $dir/available_frequencies" ;;
    esac
}

# Sets a limit's minimum and maximum, in the order that keeps minimum <= maximum throughout.
write_range() {
    local dir=$1 min_file=$2 max_file=$3 min=$4 max=$5
    local current_min
    current_min=$(on_device "cat $dir/$min_file")
    if [ "$max" -ge "$current_min" ]; then
        on_device "echo $max > $dir/$max_file && echo $min > $dir/$min_file"
    else
        on_device "echo $min > $dir/$min_file && echo $max > $dir/$max_file"
    fi
}

# Each target given on the command line, defaults filled in, the last value of a name winning:
# lines of "name khz" (macOS's bash 3.2 has no associative arrays).
targets() {
    local item name value seen=" "
    for item in $DEFAULTS "$@"; do
        name=${item%%=*}
        case $seen in *" $name "*) continue ;; esac
        seen="$seen$name "
        value=${item#*=}
        for later in "$@"; do
            [ "${later%%=*}" = "$name" ] && value=${later#*=}
        done
        [ "$value" = "none" ] || echo "$name $value"
    done
}

# The current clock of a locked target: a policy's scaling_cur_freq, the GPU's cur_freq (0 or
# its lowest step when powered off, so it isn't checked), the MIF's cur_freq.
current() {
    local dir=$1 name=$2
    case $name in
        policy*) on_device "cat $dir/scaling_cur_freq" ;;
        placement) on_device "cat $dir/uclamp_min" ;;
        *) on_device "cat $dir/cur_freq" ;;
    esac
}

check() {
    local state=$1 failed=0 name khz dir min_file max_file min max cur
    while read -r name khz; do
        read -r dir min_file max_file <<<"$(target "$name")"
        min=$(on_device "cat $dir/$min_file")
        max=$(on_device "cat $dir/$max_file")
        cur=$(current "$dir" "$name")
        if [ "$min" != "$khz" ] || [ "$max" != "$khz" ]; then
            echo "$name: min $min, max $max: not locked at $khz" >&2
            failed=1
        elif [ "${name#policy}" != "$name" ] && [ "$cur" != "$khz" ]; then
            echo "$name: running at $cur, not $khz (a thermal or other cap?)" >&2
            failed=1
        else
            echo "$name: locked at $khz (now $cur)"
        fi
    done < <(grep '^locked:' "$state" | sed 's/^locked://')
    return $failed
}

case $command in
    lock)
        [ $# -ge 1 ] || usage
        state=$1
        shift
        require_root
        if [ -e "$state" ]; then
            echo "lock_clocks.sh: $state exists: keeping its originals"
            grep -v '^locked:' "$state" > "$state.new"
            mv "$state.new" "$state"
        else
            # Every policy's and the GPU's and MIF's originals, whether locked or not.
            {
                echo "# lock_clocks.sh originals: name min max"
                for policy in $(on_device "ls $CPUFREQ | grep '^policy'"); do
                    echo "original:$policy $(on_device "cat $CPUFREQ/$policy/scaling_min_freq") $(on_device "cat $CPUFREQ/$policy/scaling_max_freq")"
                done
                gpu=$(gpu_dir)
                [ -z "$gpu" ] || echo "original:gpu $(on_device "cat $gpu/scaling_min_freq") $(on_device "cat $gpu/scaling_max_freq")"
                mif=$(mif_dir)
                [ -z "$mif" ] || echo "original:mif $(on_device "cat $mif/min_freq") $(on_device "cat $mif/max_freq")"
            } > "$state"
        fi
        while read -r name khz; do
            read -r dir min_file max_file <<<"$(target "$name")"
            if [ -z "$dir" ]; then
                echo "lock_clocks.sh: no $name found; not locked" >&2
                continue
            fi
            # A target the first lock didn't record (placement, say): its original first.
            grep -q "^original:$name " "$state" || echo "original:$name $(on_device "cat $dir/$min_file") $(on_device "cat $dir/$max_file")" >> "$state"
            if ! available "$dir" "$name" | tr ' ' '\n' | grep -qx "$khz"; then
                echo "lock_clocks.sh: $name has no $khz kHz step: $(available "$dir" "$name")" >&2
                exit 1
            fi
            write_range "$dir" "$min_file" "$max_file" "$khz" "$khz"
            echo "locked:$name $khz" >> "$state"
            grep -qx "touched:$name" "$state" || echo "touched:$name" >> "$state"
        done < <(targets "$@")
        sleep 0.2
        check "$state"
        ;;
    verify)
        [ $# -eq 1 ] || usage
        require_root
        check "$1"
        ;;
    restore)
        [ $# -eq 1 ] || usage
        state=$1
        require_root
        failed=0
        while read -r name min max; do
            # Only what a lock wrote: the power HAL moves some limits (the MIF's minimum) itself.
            grep -qx "touched:$name" "$state" || continue
            read -r dir min_file max_file <<<"$(target "$name")"
            [ -n "$dir" ] || continue
            write_range "$dir" "$min_file" "$max_file" "$min" "$max"
            now_min=$(on_device "cat $dir/$min_file")
            now_max=$(on_device "cat $dir/$max_file")
            if [ "$now_min" = "$min" ] && [ "$now_max" = "$max" ]; then
                echo "$name: restored to $min..$max"
            else
                echo "$name: $now_min..$now_max after restoring $min..$max" >&2
                # A policy's scaling_max_freq reads the effective limit, the thermal HAL's cap
                # included: a hot device reads lower until it cools.
                if [ "$now_min" = "$min" ] && [ "$now_max" -lt "$max" ]; then
                    echo "  (a thermal cap? run 'restore' again once the device is cool)" >&2
                fi
                failed=1
            fi
        done < <(grep '^original:' "$state" | sed 's/^original://')
        if [ $failed -eq 0 ]; then
            rm "$state"
        fi
        exit $failed
        ;;
    status)
        for policy in $(on_device "ls $CPUFREQ | grep '^policy'"); do
            echo "$policy: min $(on_device "cat $CPUFREQ/$policy/scaling_min_freq") max $(on_device "cat $CPUFREQ/$policy/scaling_max_freq") cur $(on_device "cat $CPUFREQ/$policy/scaling_cur_freq")"
        done
        gpu=$(gpu_dir)
        [ -z "$gpu" ] || echo "gpu: min $(on_device "cat $gpu/scaling_min_freq") max $(on_device "cat $gpu/scaling_max_freq") cur $(on_device "cat $gpu/cur_freq")"
        mif=$(mif_dir)
        [ -z "$mif" ] || echo "mif: min $(on_device "cat $mif/min_freq") max $(on_device "cat $mif/max_freq") cur $(on_device "cat $mif/cur_freq")"
        echo "placement (top-app uclamp_min): $(on_device 'cat /proc/vendor_sched/groups/ta/uclamp_min 2>/dev/null')"
        ;;
    *)
        usage
        ;;
esac
