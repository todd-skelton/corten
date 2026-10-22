<?php

namespace App\Events;

use Illuminate\Broadcasting\Channel;
use Illuminate\Contracts\Broadcasting\ShouldBroadcastNow;

/** A listing's new price, sent to every subscriber of the public `prices` channel through Reverb. */
class PriceTick implements ShouldBroadcastNow
{
    public function __construct(public int $seq, public float $sent, public string $price) {}

    public function broadcastOn(): Channel
    {
        return new Channel('prices');
    }

    public function broadcastAs(): string
    {
        return 'tick';
    }
}
