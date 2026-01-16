import React, { useState, useMemo, lazy, Suspense } from 'react';

const Pluot = lazy(async () => {
    return (await import('pluot-wrapper')).Pluot;
});

console.log(Pluot)
export function Another(props) {
    

    

    return (
        <Suspense fallback={<p>Loading Pluot...</p>}>
            <Pluot />
        </Suspense>
    );
}